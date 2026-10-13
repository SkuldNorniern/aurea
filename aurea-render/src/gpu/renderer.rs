//! The GPU renderer shared by every backend.

use aurea_foundation::{AureaError, AureaResult};

use crate::capabilities::{Fallback, GpuFeatures};
use crate::cpu::CpuRasterizer;
use crate::gpu::batch::RenderBatches;
use crate::numeric::f32_to_u32_clamped;
use crate::recording::RecordingContext;
use crate::renderer::{DrawingContext, FrameOutcome, Renderer};
use crate::scene::command::DrawCommand;
use crate::scene::display_list::{CacheKey, DisplayIndex, DisplayItem, DisplayList};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::{BlendMode, Color, Image, Rect, Sampling};

use super::backend::Gpu2dBackend;
use super::frame_plan::FramePlan;
use super::resolve::resolve_frame;
use super::texture_cache::TextureCache;

/// A 2D GPU renderer parameterized over its device backend.
pub struct Gpu2dRenderer<B: Gpu2dBackend> {
    backend: B,
    display_list: DisplayList,
    batches: RenderBatches,
    texture_cache: TextureCache,
    frame_plan: FramePlan,
    frame_counter: u64,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
    configured: (u32, u32),
    fallback: Fallback,
    cpu: Option<CpuRasterizer>,
    cpu_rgba: Vec<u8>,
    last_on_cpu: bool,
    warned_about_dropped: bool,
    layers: Vec<CpuRasterizer>,
    layer_lists: Vec<DisplayList>,
    layer_images: Vec<Option<Image>>,
    composite: DisplayList,
}

impl<B: Gpu2dBackend> Gpu2dRenderer<B> {
    /// Wrap a constructed backend.
    pub fn from_backend(backend: B, width: u32, height: u32, scale_factor: f32) -> Self {
        Self {
            backend,
            display_list: DisplayList::new(),
            batches: RenderBatches::default(),
            texture_cache: TextureCache::new(),
            frame_plan: FramePlan::new(),
            frame_counter: 0,
            warned_about_dropped: false,
            fallback: Fallback::default(),
            cpu: None,
            cpu_rgba: Vec::new(),
            last_on_cpu: false,
            layers: Vec::new(),
            layer_lists: Vec::new(),
            layer_images: Vec::new(),
            composite: DisplayList::new(),
            logical_width: width,
            logical_height: height,
            scale_factor: scale_factor.max(1.0),
            configured: physical(width, height, scale_factor.max(1.0)),
        }
    }

    fn suspended(&self) -> bool {
        self.logical_width == 0 || self.logical_height == 0
    }

    /// What happens to a frame the backend cannot draw all of; by default it is drawn on the CPU
    /// and shown through the backend.
    pub fn set_fallback(&mut self, fallback: Fallback) {
        self.fallback = fallback;
    }

    /// The kinds of drawing the last frame used that the backend lacks.
    pub fn last_frame_missing(&self) -> GpuFeatures {
        self.backend.features().missing(self.batches.needs)
    }

    fn present_on_cpu(&mut self) -> AureaResult<FrameOutcome> {
        let has_textures = self
            .display_list
            .items()
            .iter()
            .any(|i| matches!(i.command, DrawCommand::DrawTexture(..)));
        if has_textures && self.backend.features().textures {
            return self.present_layered();
        }
        let (lw, lh, scale) = (self.logical_width, self.logical_height, self.scale_factor);
        let cpu = self.cpu.get_or_insert_with(|| CpuRasterizer::new(lw, lh));
        if cpu.logical_size() != (lw, lh) || cpu.scale_factor() != scale {
            cpu.configure(SurfaceInfo {
                width: lw,
                height: lh,
                scale_factor: scale,
            })?;
        }
        let outcome = cpu.render_recorded(&mut self.display_list)?;
        if outcome == FrameOutcome::Unchanged && self.last_on_cpu {
            return Ok(FrameOutcome::Unchanged);
        }
        let Some(frame) = cpu.frame_output() else {
            return Ok(FrameOutcome::Retry);
        };
        let (width, height) = (frame.width, frame.height);
        let bgra = frame.bytes();
        self.cpu_rgba.clear();
        self.cpu_rgba.reserve(bgra.len());
        let (pixels, _) = bgra.as_chunks::<4>();
        for &[b, g, r, a] in pixels {
            self.cpu_rgba.extend_from_slice(&[r, g, b, a]);
        }
        let presented = self.backend.present_pixels(&self.cpu_rgba, width, height);
        self.last_on_cpu = presented.as_ref().is_ok_and(|o| !o.needs_redraw());
        presented
    }

    fn present_layered(&mut self) -> AureaResult<FrameOutcome> {
        self.split_at_textures();
        let (lw, lh, scale) = (self.logical_width, self.logical_height, self.scale_factor);
        while self.layers.len() < self.layer_lists.len() {
            self.layers.push(CpuRasterizer::new(lw, lh));
            self.layer_images.push(None);
        }
        self.composite.clear();
        let mut textures = self
            .display_list
            .items()
            .iter()
            .filter(|i| matches!(i.command, DrawCommand::DrawTexture(..)));
        for (k, list) in self.layer_lists.iter_mut().enumerate() {
            if k > 0
                && let Some(texture) = textures.next()
            {
                self.composite.push(texture.clone());
            }
            let layer = &mut self.layers[k];
            if layer.logical_size() != (lw, lh) || layer.scale_factor() != scale {
                layer.configure(SurfaceInfo {
                    width: lw,
                    height: lh,
                    scale_factor: scale,
                })?;
            }
            let kept = &mut self.layer_images[k];
            if layer.render_recorded(list)? != FrameOutcome::Unchanged || kept.is_none() {
                *kept = layer_image(layer);
            }
            if let Some(image) = kept.clone() {
                let full = Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
                self.composite.push(plain_item(DrawCommand::DrawImageRect(
                    image,
                    full,
                    Sampling::Nearest,
                )));
            }
        }
        self.last_on_cpu = true;
        self.batches.lower_into(&self.composite);
        self.present_batches()
    }

    fn split_at_textures(&mut self) {
        let mut count = 0;
        let start_layer = |lists: &mut Vec<DisplayList>, count: &mut usize| {
            if lists.len() <= *count {
                lists.push(DisplayList::new());
            }
            let list = &mut lists[*count];
            list.clear();
            if *count > 0 {
                list.push(plain_item(DrawCommand::Clear(Color::rgba(0, 0, 0, 0))));
            }
            *count += 1;
        };
        start_layer(&mut self.layer_lists, &mut count);
        for item in self.display_list.items() {
            if matches!(item.command, DrawCommand::DrawTexture(..)) {
                start_layer(&mut self.layer_lists, &mut count);
            } else {
                self.layer_lists[count - 1].push(item.clone());
            }
        }
        self.layer_lists.truncate(count);
    }

    fn present_batches(&mut self) -> AureaResult<FrameOutcome> {
        let (pw, ph) = self.physical_size();
        self.frame_plan.viewport_width = pw;
        self.frame_plan.viewport_height = ph;
        self.frame_counter += 1;
        let fc = self.frame_counter;
        let presented = resolve_frame(
            &self.batches,
            &mut self.texture_cache,
            &mut self.backend,
            fc,
            &mut self.frame_plan,
        )
        .and_then(|()| {
            let rects = &self.batches.rects;
            let shapes = &self.batches.shapes;
            self.backend.present_frame(&self.frame_plan, rects, shapes)
        });
        recover(presented)
    }

    /// Shared access to the device backend (for backend-specific extensions such as engine-side
    /// image embedding via `draw_sampled_image`).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable access to the device backend.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    fn physical_size(&self) -> (u32, u32) {
        physical(self.logical_width, self.logical_height, self.scale_factor)
    }

    fn report_dropped(&mut self) {
        if self.batches.dropped == 0 || self.warned_about_dropped {
            return;
        }
        self.warned_about_dropped = true;
        log::warn!(
            "GPU backend has no form for {} draw(s) in this frame; they are              missing from it. Paths, non-normal blend modes and the legacy              text commands are not implemented here yet — a canvas needing              them wants the CPU renderer.",
            self.batches.dropped,
        );
    }
}

impl<B: Gpu2dBackend> Renderer for Gpu2dRenderer<B> {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.configure(info)
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.configure(SurfaceInfo {
            width,
            height,
            scale_factor: self.scale_factor,
        })
    }

    fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()> {
        self.logical_width = info.width;
        self.logical_height = info.height;
        self.scale_factor = info.scale_factor.max(1.0);
        self.display_list.clear();
        if self.suspended() {
            return Ok(());
        }
        let size = self.physical_size();
        if size != self.configured {
            self.backend.resize(size.0, size.1)?;
            self.configured = size;
        }
        Ok(())
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext + '_>> {
        self.backend.begin_frame()?;
        self.texture_cache.prune_dropped(&mut self.backend);
        self.display_list.clear();
        let mut ctx = RecordingContext::new(
            &mut self.display_list,
            self.logical_width,
            self.logical_height,
        );
        ctx.set_scale_factor(self.scale_factor);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<FrameOutcome> {
        if self.suspended() {
            return Ok(FrameOutcome::Suspended);
        }
        self.batches.lower_into(&self.display_list);
        let missing = self.backend.features().missing(self.batches.needs);
        if !missing.is_empty() && self.fallback == Fallback::CpuFrame {
            return self.present_on_cpu();
        }
        self.last_on_cpu = false;
        self.report_dropped();
        self.present_batches()
    }

    fn cleanup(&mut self) {
        self.display_list.clear();
    }

    fn set_damage(&mut self, _damage: Option<Rect>) {}

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }
}

fn plain_item(command: DrawCommand) -> DisplayItem {
    DisplayItem::new(
        DisplayIndex(0),
        CacheKey::from_hash(0),
        Rect::new(0.0, 0.0, 0.0, 0.0),
        false,
        BlendMode::Normal,
        command,
    )
}

fn layer_image(layer: &CpuRasterizer) -> Option<Image> {
    let frame = layer.frame_output()?;
    let (pixels, _) = frame.bytes().as_chunks::<4>();
    let rgba = pixels
        .iter()
        .flat_map(|&[b, g, r, a]| [r, g, b, a])
        .collect();
    Some(Image::new(frame.width, frame.height, rgba))
}

fn recover(outcome: AureaResult<FrameOutcome>) -> AureaResult<FrameOutcome> {
    match outcome {
        Err(AureaError::DeviceLost) => Ok(FrameOutcome::Recreate),
        other => other,
    }
}

fn physical(width: u32, height: u32, scale: f32) -> (u32, u32) {
    (
        f32_to_u32_clamped((width as f32 * scale).round()).max(1),
        f32_to_u32_clamped((height as f32 * scale).round()).max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::batch::{DrawRef, RectInstance, ShapeInstance};
    use crate::types::GpuTexture;
    use crate::types::{Color, Paint};

    struct Probe {
        resized: Vec<(u32, u32)>,
        outcome: FrameOutcome,
        features: GpuFeatures,
        drawn: usize,
        pixels: Vec<(u32, u32, [u8; 4])>,
        orders: Vec<Vec<DrawRef>>,
        uploads: usize,
        bound: usize,
    }

    impl Gpu2dBackend for Probe {
        fn resize(&mut self, w: u32, h: u32) -> AureaResult<()> {
            self.resized.push((w, h));
            Ok(())
        }

        fn upload_image(&mut self, _: u32, _: u32, _: &[u8]) -> AureaResult<u32> {
            self.uploads += 1;
            Ok(0)
        }

        fn bind_texture(&mut self, texture: GpuTexture) -> AureaResult<u32> {
            self.bound += 1;
            Ok(texture.index)
        }

        fn features(&self) -> GpuFeatures {
            self.features
        }

        fn evict_image(&mut self, _: u32) {}

        fn present_frame(
            &mut self,
            plan: &FramePlan,
            _: &[RectInstance],
            _: &[ShapeInstance],
        ) -> AureaResult<FrameOutcome> {
            self.drawn += 1;
            self.orders.push(plan.order.clone());
            Ok(self.outcome)
        }

        fn present_pixels(&mut self, rgba: &[u8], w: u32, h: u32) -> AureaResult<FrameOutcome> {
            let at = ((h / 2 * w + w / 2) * 4) as usize;
            let px = [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]];
            self.pixels.push((w, h, px));
            Ok(FrameOutcome::Presented)
        }
    }

    fn renderer(outcome: FrameOutcome) -> Gpu2dRenderer<Probe> {
        with_features(outcome, GpuFeatures::ALL)
    }

    fn with_features(outcome: FrameOutcome, features: GpuFeatures) -> Gpu2dRenderer<Probe> {
        let probe = Probe {
            resized: Vec::new(),
            outcome,
            features,
            drawn: 0,
            pixels: Vec::new(),
            orders: Vec::new(),
            uploads: 0,
            bound: 0,
        };
        Gpu2dRenderer::from_backend(probe, 100, 50, 1.0)
    }

    fn rounded_frame(r: &mut Gpu2dRenderer<Probe>) -> FrameOutcome {
        {
            let mut ctx = r.begin_frame().expect("begin");
            let paint = Paint::new().color(Color::rgb(220, 20, 60));
            ctx.draw_round_rect(Rect::new(0.0, 0.0, 100.0, 50.0), 8.0, &paint)
                .expect("draw");
        }
        r.end_frame().expect("end")
    }

    #[test]
    fn a_cpu_frame_keeps_its_textures_in_order() {
        let features = GpuFeatures {
            rounded_rects: false,
            textures: true,
            ..GpuFeatures::ALL
        };
        let mut r = with_features(FrameOutcome::Presented, features);
        let texture = GpuTexture {
            index: 9,
            generation: 0,
            width: 4,
            height: 4,
        };
        let frame = |r: &mut Gpu2dRenderer<Probe>| {
            let mut ctx = r.begin_frame().expect("begin");
            let paint = Paint::new().color(Color::rgb(220, 20, 60));
            ctx.draw_rect(Rect::new(0.0, 0.0, 100.0, 50.0), &paint)
                .expect("rect");
            let all = Rect::new(0.0, 0.0, 1.0, 1.0);
            ctx.draw_texture(texture, all, Rect::new(10.0, 10.0, 20.0, 20.0))
                .expect("texture");
            ctx.draw_round_rect(Rect::new(5.0, 5.0, 30.0, 30.0), 4.0, &paint)
                .expect("rounded");
            drop(ctx);
            r.end_frame().expect("end")
        };
        frame(&mut r);
        let probe = r.backend();
        assert!(probe.pixels.is_empty(), "not a flat CPU frame");
        assert_eq!(
            probe.orders[0],
            [DrawRef::Image(0), DrawRef::Image(1), DrawRef::Image(2)]
        );
        assert_eq!((probe.bound, probe.uploads), (1, 2));
        frame(&mut r);
        assert_eq!(
            r.backend().uploads,
            2,
            "unchanged layers are not uploaded again"
        );
    }

    #[test]
    fn a_frame_the_backend_cannot_draw_goes_through_the_cpu() {
        let no_rounding = GpuFeatures {
            rounded_rects: false,
            ..GpuFeatures::ALL
        };
        let mut r = with_features(FrameOutcome::Presented, no_rounding);
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Presented);
        assert_eq!(r.backend().drawn, 0, "nothing is handed over half drawn");
        assert_eq!(r.backend().pixels, vec![(100, 50, [220, 20, 60, 255])]);
        assert!(r.last_frame_missing().rounded_rects);
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Unchanged);
        assert_eq!(r.backend().pixels.len(), 1);
    }

    #[test]
    fn a_frame_within_the_backend_is_drawn_on_the_gpu() {
        let mut r = renderer(FrameOutcome::Presented);
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Presented);
        assert_eq!(r.backend().drawn, 1);
        assert!(r.backend().pixels.is_empty());
    }

    #[test]
    fn partial_fallback_draws_what_it_can() {
        let mut r = with_features(FrameOutcome::Presented, GpuFeatures::NONE);
        r.set_fallback(Fallback::Partial);
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Presented);
        assert_eq!(r.backend().drawn, 1);
        assert!(r.backend().pixels.is_empty());
    }

    fn info(width: u32, height: u32, scale_factor: f32) -> SurfaceInfo {
        SurfaceInfo {
            width,
            height,
            scale_factor,
        }
    }

    #[test]
    fn a_scale_change_alone_resizes_the_surface() {
        let mut r = renderer(FrameOutcome::Presented);
        r.configure(info(100, 50, 2.0)).expect("configure");
        assert_eq!(r.backend().resized, vec![(200, 100)]);
        r.configure(info(100, 50, 2.0)).expect("configure");
        assert_eq!(r.backend().resized.len(), 1);
    }

    #[test]
    fn no_size_suspends_until_there_is_one() {
        let mut r = renderer(FrameOutcome::Presented);
        r.configure(info(0, 50, 1.0)).expect("configure");
        drop(r.begin_frame().expect("begin"));
        assert_eq!(r.end_frame().expect("end"), FrameOutcome::Suspended);
        assert!(r.backend().resized.is_empty());
        r.configure(info(80, 50, 1.0)).expect("configure");
        drop(r.begin_frame().expect("begin"));
        assert_eq!(r.end_frame().expect("end"), FrameOutcome::Presented);
    }

    #[test]
    fn a_lost_device_asks_for_a_new_renderer() {
        assert_eq!(
            recover(Err(AureaError::DeviceLost)).ok(),
            Some(FrameOutcome::Recreate)
        );
        assert!(recover(Err(AureaError::RenderingFailed)).is_err());
        assert_eq!(
            recover(Ok(FrameOutcome::Presented)).ok(),
            Some(FrameOutcome::Presented)
        );
    }

    #[test]
    fn a_frame_the_surface_refused_is_reported() {
        let mut r = renderer(FrameOutcome::Retry);
        drop(r.begin_frame().expect("begin"));
        let outcome = r.end_frame().expect("end");
        assert_eq!(outcome, FrameOutcome::Retry);
        assert!(outcome.needs_redraw());
    }
}
