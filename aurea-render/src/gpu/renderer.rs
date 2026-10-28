//! The GPU renderer shared by every backend.

use std::sync::Arc;

use aurea_foundation::{AureaError, AureaResult};

use crate::capabilities::{Fallback, GpuFeatures};
use crate::cpu::CpuRasterizer;
use crate::frame::FrameOutput;
use crate::gpu::batch::RenderBatches;
use crate::memory::smaller_buffer;
use crate::recording::RecordingContext;
use crate::renderer::{DrawingContext, FrameOutcome, Renderer};
use crate::scene::command::DrawCommand;
use crate::scene::display_list::{CacheKey, DisplayIndex, DisplayItem, DisplayList};
use crate::surface::{Surface, SurfaceInfo, pixel_size};
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
    pixels: Option<(u32, u32)>,
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
    presented: Vec<(CacheKey, Rect)>,
    presented_target: Option<PresentedTarget>,
}

/// The target a presented frame was drawn to; any change to it means the frame must be presented again.
#[derive(Clone, Copy, PartialEq)]
struct PresentedTarget {
    logical: (u32, u32),
    scale: f32,
    pixels: Option<(u32, u32)>,
    physical: (u32, u32),
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
            presented: Vec::new(),
            presented_target: None,
            logical_width: width,
            logical_height: height,
            scale_factor: scale_factor.max(1.0),
            pixels: None,
            configured: pixel_size(width, height, scale_factor, None),
        }
    }

    fn suspended(&self) -> bool {
        self.logical_width == 0 || self.logical_height == 0
    }

    fn target_now(&self) -> PresentedTarget {
        PresentedTarget {
            logical: (self.logical_width, self.logical_height),
            scale: self.scale_factor,
            pixels: self.pixels,
            physical: self.physical_size(),
        }
    }

    /// Whether the display list draws exactly what the target already shows.
    fn matches_presented(&self) -> bool {
        if self.presented_target != Some(self.target_now()) {
            return false;
        }
        let items = self.display_list.items();
        // An app texture can change in place under the same handle, so a frame drawing one is
        // always presented.
        if items.iter().any(|item| {
            matches!(
                item.command,
                DrawCommand::DrawTexture(..) | DrawCommand::DrawTextureQuad(..)
            )
        }) {
            return false;
        }
        items.len() == self.presented.len()
            && items
                .iter()
                .zip(&self.presented)
                .all(|(item, &(key, bounds))| item.cache_key == key && item.bounds == bounds)
    }

    fn remember_presented(&mut self) {
        self.presented_target = Some(self.target_now());
        self.presented.clear();
        self.presented.extend(
            self.display_list
                .items()
                .iter()
                .map(|item| (item.cache_key, item.bounds)),
        );
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
        let has_textures = self.display_list.items_since_clear().iter().any(|i| {
            matches!(
                i.command,
                DrawCommand::DrawTexture(..) | DrawCommand::DrawTextureQuad(..)
            )
        });
        if has_textures && self.backend.features().textures {
            return self.present_layered();
        }
        self.release_layers();
        let (lw, lh, scale) = (self.logical_width, self.logical_height, self.scale_factor);
        let info = self.surface_info();
        let cpu = self.cpu.get_or_insert_with(|| CpuRasterizer::new(lw, lh));
        if cpu.logical_size() != (lw, lh)
            || cpu.scale_factor() != scale
            || cpu.pixel_size() != info.pixel_size()
        {
            cpu.configure(info)?;
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
        if outcome != FrameOutcome::Unchanged || self.cpu_rgba.len() != bgra.len() {
            if let Some(compact) = smaller_buffer(self.cpu_rgba.capacity(), bgra.len()) {
                self.cpu_rgba = compact;
            }
            self.cpu_rgba.clear();
            self.cpu_rgba.reserve(bgra.len());
            let (pixels, _) = bgra.as_chunks::<4>();
            for &[b, g, r, a] in pixels {
                self.cpu_rgba.extend_from_slice(&[r, g, b, a]);
            }
        }
        let presented = self.backend.present_pixels(&self.cpu_rgba, width, height);
        self.last_on_cpu = presented.as_ref().is_ok_and(|o| !o.needs_redraw());
        presented
    }

    fn present_layered(&mut self) -> AureaResult<FrameOutcome> {
        self.split_at_textures();
        self.layers.truncate(self.layer_lists.len());
        self.layer_images.truncate(self.layer_lists.len());
        let (lw, lh, scale) = (self.logical_width, self.logical_height, self.scale_factor);
        while self.layers.len() < self.layer_lists.len() {
            self.layers.push(CpuRasterizer::new(lw, lh));
            self.layer_images.push(None);
        }
        self.composite.clear();
        let mut textures = self.display_list.items_since_clear().iter().filter(|i| {
            matches!(
                i.command,
                DrawCommand::DrawTexture(..) | DrawCommand::DrawTextureQuad(..)
            )
        });
        for (k, list) in self.layer_lists.iter_mut().enumerate() {
            if k > 0
                && let Some(texture) = textures.next()
            {
                self.composite.push(texture.clone());
            }
            let layer = &mut self.layers[k];
            let info = SurfaceInfo {
                width: lw,
                height: lh,
                scale_factor: scale,
                pixels: self.pixels,
            };
            if layer.logical_size() != (lw, lh)
                || layer.scale_factor() != scale
                || layer.pixel_size() != info.pixel_size()
            {
                layer.configure(info)?;
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
        self.last_on_cpu = false;
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
        for item in self.display_list.items_since_clear() {
            if matches!(
                item.command,
                DrawCommand::DrawTexture(..) | DrawCommand::DrawTextureQuad(..)
            ) {
                start_layer(&mut self.layer_lists, &mut count);
            } else {
                self.layer_lists[count - 1].push(item.clone());
            }
        }
        self.layer_lists.truncate(count);
    }

    fn release_layers(&mut self) {
        self.layers.clear();
        self.layer_images.clear();
        self.layer_lists.clear();
        self.composite.clear();
    }

    fn present_batches(&mut self) -> AureaResult<FrameOutcome> {
        let (pw, ph) = self.physical_size();
        self.frame_plan.viewport_width = pw;
        self.frame_plan.viewport_height = ph;
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
        self.surface_info().pixel_size()
    }

    fn surface_info(&self) -> SurfaceInfo {
        SurfaceInfo {
            width: self.logical_width,
            height: self.logical_height,
            scale_factor: self.scale_factor,
            pixels: self.pixels,
        }
    }

    fn report_dropped(&mut self) {
        if self.batches.dropped == 0 || self.warned_about_dropped {
            return;
        }
        self.warned_about_dropped = true;
        log::warn!(
            "GPU backend has no form for {} draw(s) in this frame; they are missing from it. Paths, non-normal blend modes and the legacy text commands are not implemented here yet — a canvas needing them wants the CPU renderer.",
            self.batches.dropped,
        );
    }
}

impl<B: Gpu2dBackend> Renderer for Gpu2dRenderer<B> {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.configure(info)
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.configure(SurfaceInfo::new(width, height, self.scale_factor))
    }

    fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()> {
        self.logical_width = info.width;
        self.logical_height = info.height;
        self.scale_factor = info.scale_factor.max(1.0);
        self.pixels = info.pixels;
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
        // the backend draws none; a frame with one goes to the CPU as the fallback says
        ctx.set_path_clips(true);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<FrameOutcome> {
        if self.suspended() {
            self.presented_target = None;
            return Ok(FrameOutcome::Suspended);
        }
        if self.matches_presented() {
            return Ok(FrameOutcome::Unchanged);
        }
        self.frame_counter += 1;
        self.batches.lower_into(&self.display_list);
        let missing = self.backend.features().missing(self.batches.needs);
        let presented = if !missing.is_empty() && self.fallback == Fallback::CpuFrame {
            self.present_on_cpu()
        } else {
            self.release_layers();
            self.last_on_cpu = false;
            self.report_dropped();
            self.present_batches()
        };
        let presented = recover(presented);
        if presented.is_ok() {
            self.texture_cache.prune_dropped(&mut self.backend);
            self.texture_cache
                .finish_frame(&mut self.backend, self.frame_counter);
        }
        match &presented {
            Ok(FrameOutcome::Presented) => self.remember_presented(),
            Ok(FrameOutcome::Unchanged) => {}
            _ => self.presented_target = None,
        }
        presented
    }

    fn cleanup(&mut self) {
        self.display_list.clear();
    }

    fn set_damage(&mut self, _damage: Option<Rect>) {}

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }

    fn features(&self) -> GpuFeatures {
        self.backend.features()
    }

    fn frame_output(&self) -> Option<FrameOutput<'_>> {
        let (words, width, height) = self.backend.frame_pixels()?;
        FrameOutput::new(words, width, height)
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

/// Source offset in a BGRA pixel for each RGBA output channel.
const BGRA_TO_RGBA: [usize; 4] = [2, 1, 0, 3];

fn layer_image(layer: &CpuRasterizer) -> Option<Image> {
    let frame = layer.frame_output()?;
    let bgra = frame.bytes();
    // A range map has an exact length, so the pixels are written once, into the shared buffer.
    let rgba: Arc<[u8]> = (0..bgra.len())
        .map(|i| bgra[(i & !3) | BGRA_TO_RGBA[i & 3]])
        .collect();
    Some(Image::from_shared(frame.width, frame.height, rgba))
}

fn recover(outcome: AureaResult<FrameOutcome>) -> AureaResult<FrameOutcome> {
    match outcome {
        Err(AureaError::DeviceLost) => Ok(FrameOutcome::Recreate),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::batch::{DrawRef, RectInstance, ShapeInstance};
    use crate::types::GpuTexture;
    use crate::types::{Color, Paint, Path, PathCommand, Point};
    use std::sync::Arc;
    #[cfg(feature = "zengpu")]
    use zengpu_hal::TextureHandle;

    struct Probe {
        resized: Vec<(u32, u32)>,
        outcome: FrameOutcome,
        features: GpuFeatures,
        drawn: usize,
        pixels: Vec<(u32, u32, [u8; 4])>,
        orders: Vec<Vec<DrawRef>>,
        uploads: usize,
        evicted: usize,
        bound: usize,
        pixels_device_lost: bool,
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

        fn evict_image(&mut self, _: u32) {
            self.evicted += 1;
        }

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
            if self.pixels_device_lost {
                return Err(AureaError::DeviceLost);
            }
            let at = ((h / 2 * w + w / 2) * 4) as usize;
            let px = [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]];
            self.pixels.push((w, h, px));
            Ok(self.outcome)
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
            evicted: 0,
            bound: 0,
            pixels_device_lost: false,
        };
        Gpu2dRenderer::from_backend(probe, 100, 50, 1.0)
    }

    #[test]
    fn smaller_gpu_and_cpu_frames_trim_an_oversized_texture_cache() {
        for cpu_fallback in [false, true] {
            let mut r = with_features(
                FrameOutcome::Presented,
                GpuFeatures {
                    rounded_rects: false,
                    ..GpuFeatures::ALL
                },
            );
            r.texture_cache = TextureCache::with_capacity(2);
            let images = [
                Image::new(1, 1, vec![255; 4]),
                Image::new(1, 1, vec![255; 4]),
                Image::new(1, 1, vec![255; 4]),
            ];
            {
                let mut ctx = r.begin_frame().expect("begin");
                for image in &images {
                    ctx.draw_image(image, Point::new(0.0, 0.0)).expect("image");
                }
            }
            r.end_frame().expect("large frame");
            assert_eq!(r.backend().uploads, 3);
            assert_eq!(r.backend().evicted, 0);
            {
                let mut ctx = r.begin_frame().expect("begin");
                if cpu_fallback {
                    ctx.draw_round_rect(Rect::new(0.0, 0.0, 20.0, 20.0), 2.0, &Paint::new())
                        .expect("fallback");
                } else {
                    ctx.draw_image(&images[0], Point::new(0.0, 0.0))
                        .expect("cache hit");
                }
            }
            r.end_frame().expect("smaller frame");
            assert_eq!(r.backend().uploads, 3);
            assert_eq!(r.backend().evicted, 1);
        }
    }

    #[test]
    fn path_clips_record_and_draw_on_the_cpu() {
        let mut r = with_features(
            FrameOutcome::Presented,
            GpuFeatures {
                path_clips: false,
                ..GpuFeatures::ALL
            },
        );
        {
            let mut ctx = r.begin_frame().expect("begin");
            let mut triangle = Path::new();
            triangle.commands.extend([
                PathCommand::MoveTo(Point::new(0.0, 0.0)),
                PathCommand::LineTo(Point::new(80.0, 0.0)),
                PathCommand::LineTo(Point::new(0.0, 40.0)),
                PathCommand::Close,
            ]);
            ctx.clip_path(&triangle).expect("a path clip records");
            ctx.draw_rect(Rect::new(0.0, 0.0, 100.0, 50.0), &Paint::new())
                .expect("draw");
        }
        assert_eq!(r.end_frame().expect("end"), FrameOutcome::Presented);
        assert!(r.last_frame_missing().path_clips, "drawn on the CPU");
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
            #[cfg(feature = "zengpu")]
            handle: TextureHandle::from_parts(9, 0),
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
            ctx.save().expect("save");
            ctx.rotate(0.2).expect("rotate imported texture");
            ctx.draw_texture(texture, all, Rect::new(10.0, 10.0, 20.0, 20.0))
                .expect("texture");
            ctx.restore().expect("restore");
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
        assert!(
            r.frame_plan.images[1].affine[0].abs() > 1.0,
            "rotated texture geometry survives layered resolve"
        );
        frame(&mut r);
        assert_eq!(
            r.backend().uploads,
            2,
            "unchanged layers are not uploaded again"
        );
        assert_eq!(r.layers.len(), 2);
        {
            r.begin_frame()
                .expect("begin")
                .draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), &Paint::new())
                .expect("rect");
        }
        r.end_frame().expect("GPU frame");
        assert!(r.layers.is_empty() && r.layer_images.is_empty());
        assert!(r.layer_lists.is_empty() && r.composite.items().is_empty());
    }

    #[test]
    fn returning_from_layers_presents_the_cached_cpu_frame() {
        let mut r = with_features(
            FrameOutcome::Presented,
            GpuFeatures {
                rounded_rects: false,
                textures: true,
                ..GpuFeatures::ALL
            },
        );
        let cpu_frame = |r: &mut Gpu2dRenderer<Probe>| {
            {
                r.begin_frame()
                    .expect("begin")
                    .draw_round_rect(Rect::new(0.0, 0.0, 20.0, 20.0), 3.0, &Paint::new())
                    .expect("round rect");
            }
            r.end_frame().expect("CPU frame")
        };
        assert_eq!(cpu_frame(&mut r), FrameOutcome::Presented);
        let texture = GpuTexture {
            #[cfg(feature = "zengpu")]
            handle: TextureHandle::from_parts(9, 0),
            index: 9,
            generation: 0,
            width: 4,
            height: 4,
        };
        {
            let mut ctx = r.begin_frame().expect("begin");
            ctx.draw_round_rect(Rect::new(0.0, 0.0, 20.0, 20.0), 3.0, &Paint::new())
                .expect("round rect");
            ctx.draw_texture(
                texture,
                Rect::new(0.0, 0.0, 1.0, 1.0),
                Rect::new(2.0, 2.0, 4.0, 4.0),
            )
            .expect("texture");
        }
        r.end_frame().expect("layered frame");
        assert_eq!(r.layers.len(), 2);
        assert_eq!(
            cpu_frame(&mut r),
            FrameOutcome::Presented,
            "layered pixels must be replaced"
        );
        assert_eq!(r.backend().pixels.len(), 2);
        assert!(r.layers.is_empty() && r.layer_images.is_empty());
        assert_eq!(
            cpu_frame(&mut r),
            FrameOutcome::Unchanged,
            "standalone repeats still skip presentation"
        );
    }

    #[test]
    fn fewer_embedded_textures_release_unused_cpu_layers_and_their_images() {
        let mut r = with_features(
            FrameOutcome::Presented,
            GpuFeatures {
                rounded_rects: false,
                textures: true,
                ..GpuFeatures::ALL
            },
        );
        let texture = GpuTexture {
            #[cfg(feature = "zengpu")]
            handle: TextureHandle::from_parts(9, 0),
            index: 9,
            generation: 0,
            width: 4,
            height: 4,
        };
        let draw = |r: &mut Gpu2dRenderer<Probe>, count: u16| {
            {
                let mut ctx = r.begin_frame().expect("begin");
                for _ in 0..count {
                    ctx.draw_round_rect(Rect::new(0.0, 0.0, 20.0, 20.0), 3.0, &Paint::new())
                        .expect("round rect");
                    ctx.draw_texture(
                        texture,
                        Rect::new(0.0, 0.0, 1.0, 1.0),
                        Rect::new(2.0, 2.0, 4.0, 4.0),
                    )
                    .expect("texture");
                }
            }
            r.end_frame().expect("layered frame");
        };
        draw(&mut r, 2);
        assert_eq!(r.layers.len(), 3);
        let old_image = Arc::downgrade(&r.layer_images[2].as_ref().expect("last layer").data);
        draw(&mut r, 1);
        assert_eq!(r.layers.len(), 2);
        assert_eq!(r.layer_images.len(), 2);
        assert!(
            old_image.upgrade().is_none(),
            "unused layer pixels should not remain cached"
        );
    }

    #[test]
    fn shrinking_a_cpu_fallback_frame_releases_pixel_conversion_capacity() {
        let mut r = with_features(
            FrameOutcome::Presented,
            GpuFeatures {
                rounded_rects: false,
                ..GpuFeatures::ALL
            },
        );
        let draw = |r: &mut Gpu2dRenderer<Probe>| {
            {
                r.begin_frame()
                    .expect("begin")
                    .draw_round_rect(Rect::new(0.0, 0.0, 20.0, 20.0), 3.0, &Paint::new())
                    .expect("round rect");
            }
            r.end_frame().expect("fallback frame");
        };
        r.resize(2048, 1024).expect("large resize");
        draw(&mut r);
        assert!(r.cpu_rgba.capacity() >= 8 << 20);
        r.resize(128, 128).expect("shrink");
        draw(&mut r);
        assert_eq!(r.cpu_rgba.len(), 128 * 128 * 4);
        assert!(r.cpu_rgba.capacity() <= 2 * r.cpu_rgba.len());
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
            pixels: None,
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

    #[test]
    fn cpu_presentation_device_loss_requests_recreation_and_does_not_skip_retry() {
        let mut r = with_features(FrameOutcome::Presented, GpuFeatures::NONE);
        r.backend_mut().pixels_device_lost = true;
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Recreate);
        assert!(!r.last_on_cpu);
        r.backend_mut().pixels_device_lost = false;
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Presented);
        assert_eq!(r.backend().pixels, vec![(100, 50, [220, 20, 60, 255])]);
    }

    #[test]
    fn refused_cpu_frames_are_presented_again_until_they_reach_the_surface() {
        let mut r = with_features(FrameOutcome::Retry, GpuFeatures::NONE);
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Retry);
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Retry);
        r.backend_mut().outcome = FrameOutcome::Presented;
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Presented);
        assert_eq!(rounded_frame(&mut r), FrameOutcome::Unchanged);
        assert_eq!(r.backend().pixels, vec![(100, 50, [220, 20, 60, 255]); 3]);
    }

    #[test]
    fn clearing_a_texture_before_cpu_fallback_does_not_bind_it_or_keep_layers() {
        let mut r = with_features(
            FrameOutcome::Presented,
            GpuFeatures {
                rounded_rects: false,
                ..GpuFeatures::ALL
            },
        );
        let texture = GpuTexture {
            #[cfg(feature = "zengpu")]
            handle: TextureHandle::from_parts(9, 0),
            index: 9,
            generation: 0,
            width: 4,
            height: 4,
        };
        {
            let mut ctx = r.begin_frame().expect("begin");
            ctx.draw_texture(
                texture,
                Rect::new(0.0, 0.0, 1.0, 1.0),
                Rect::new(0.0, 0.0, 4.0, 4.0),
            )
            .expect("texture");
            ctx.clear(Color::rgb(10, 20, 30)).expect("clear");
            ctx.draw_round_rect(
                Rect::new(0.0, 0.0, 100.0, 50.0),
                3.0,
                &Paint::new().color(Color::rgb(220, 20, 60)),
            )
            .expect("fallback");
        }
        assert_eq!(r.end_frame().expect("end"), FrameOutcome::Presented);
        assert_eq!(r.backend().bound, 0);
        assert!(r.layers.is_empty());
        assert_eq!(r.backend().pixels, vec![(100, 50, [220, 20, 60, 255])]);
    }

    fn rect_frame(r: &mut Gpu2dRenderer<Probe>, width: f32) -> FrameOutcome {
        {
            let mut ctx = r.begin_frame().expect("begin");
            ctx.draw_rect(Rect::new(0.0, 0.0, width, 10.0), &Paint::new())
                .expect("rect");
        }
        r.end_frame().expect("end")
    }

    #[test]
    fn an_identical_frame_is_not_submitted_again() {
        let mut r = renderer(FrameOutcome::Presented);
        assert_eq!(rect_frame(&mut r, 20.0), FrameOutcome::Presented);
        assert_eq!(rect_frame(&mut r, 20.0), FrameOutcome::Unchanged);
        assert_eq!(r.backend().drawn, 1);
        assert_eq!(rect_frame(&mut r, 30.0), FrameOutcome::Presented);
        assert_eq!(r.backend().drawn, 2);
        r.configure(info(80, 50, 1.0)).expect("configure");
        assert_eq!(rect_frame(&mut r, 30.0), FrameOutcome::Presented);
        assert_eq!(rect_frame(&mut r, 30.0), FrameOutcome::Unchanged);
        assert_eq!(r.backend().drawn, 3);
    }

    #[test]
    fn a_frame_the_surface_refused_is_presented_again() {
        let mut r = renderer(FrameOutcome::Retry);
        assert_eq!(rect_frame(&mut r, 20.0), FrameOutcome::Retry);
        r.backend_mut().outcome = FrameOutcome::Presented;
        assert_eq!(rect_frame(&mut r, 20.0), FrameOutcome::Presented);
        assert_eq!(rect_frame(&mut r, 20.0), FrameOutcome::Unchanged);
    }

    #[test]
    fn a_texture_is_bound_once_per_handle_and_generation() {
        let texture = |generation: u32| GpuTexture {
            #[cfg(feature = "zengpu")]
            handle: TextureHandle::from_parts(9, generation),
            index: 9,
            generation,
            width: 4,
            height: 4,
        };
        let mut r = with_features(
            FrameOutcome::Presented,
            GpuFeatures {
                textures: true,
                ..GpuFeatures::ALL
            },
        );
        let draw = |r: &mut Gpu2dRenderer<Probe>, texture: GpuTexture| {
            {
                let mut ctx = r.begin_frame().expect("begin");
                for _ in 0..2 {
                    ctx.draw_texture(
                        texture,
                        Rect::new(0.0, 0.0, 1.0, 1.0),
                        Rect::new(0.0, 0.0, 4.0, 4.0),
                    )
                    .expect("texture");
                }
            }
            r.end_frame().expect("end")
        };
        draw(&mut r, texture(0));
        assert_eq!(r.backend().bound, 1);
        draw(&mut r, texture(0));
        assert_eq!(r.backend().bound, 1, "a known handle keeps its slot");
        draw(&mut r, texture(1));
        assert_eq!(r.backend().bound, 2, "a reused index binds again");
    }

    #[test]
    fn a_layer_image_is_rgba_of_what_the_layer_drew() {
        let mut layer = CpuRasterizer::new(2, 2);
        layer.configure(info(2, 2, 1.0)).expect("configure");
        let mut list = DisplayList::new();
        {
            let mut ctx = RecordingContext::new(&mut list, 2, 2);
            ctx.draw_rect(
                Rect::new(0.0, 0.0, 2.0, 2.0),
                &Paint::new().color(Color::rgb(220, 20, 60)),
            )
            .expect("rect");
        }
        layer.render_recorded(&mut list).expect("render");
        let image = layer_image(&layer).expect("image");
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(image.data.len(), 2 * 2 * 4);
        assert_eq!(&image.data[..4], &[220, 20, 60, 255]);
    }
}
