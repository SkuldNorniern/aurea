//! Backend-agnostic GPU-2D rendering core.
//!
//! [`Gpu2dRenderer`] implements aurea's [`Renderer`] trait **once**, over any
//! [`Gpu2dBackend`]. It owns the display list, batch lowering, texture-cache
//! LRU policy, and display-list resolution; the backend supplies only
//! device-touching primitives. ZenGPU is the first backend on this core; wgpu
//! follows on the same seam, so cache/record machinery exists once instead of
//! once per backend.
//!
//! Laid out like the modular `cpu/` backend: one concern per piece, a thin
//! device boundary, and "add a backend = implement one trait".
//!
//! ZenGPU is the backend Aurea is built around and where GPU work goes. wgpu
//! is here so Aurea can render inside an application already built on wgpu,
//! which is a different job from being the fast path. What they share is the
//! lowering above this line, not a ceiling: a shared trait should not stop
//! ZenGPU using something wgpu cannot express.

mod backend;
mod frame_plan;
mod resolve;
mod texture_cache;

pub use backend::Gpu2dBackend;
pub use frame_plan::FramePlan;

use aurea_foundation::AureaResult;

use crate::batch::RenderBatches;
use crate::cpu::RecordingContext;
use crate::display_list::DisplayList;
use crate::numeric::f32_to_u32_clamped;
use crate::renderer::{DrawingContext, FrameOutcome, Renderer};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::Rect;

use resolve::resolve_frame;
use texture_cache::TextureCache;

/// A 2D GPU renderer parameterized over its device backend.
///
/// Records draw calls into a [`DisplayList`] (the same [`RecordingContext`]
/// the CPU rasterizer uses), lowers them to [`RenderBatches`] in `end_frame`,
/// resolves textures through the backend-agnostic [`TextureCache`], and hands
/// the resulting [`FramePlan`] to the backend to upload, record, and present.
/// Holds no `vk`/`wgpu` types.
pub struct Gpu2dRenderer<B: Gpu2dBackend> {
    backend: B,
    display_list: DisplayList,
    /// Reused across frames so steady-state `end_frame` allocates nothing.
    batches: RenderBatches,
    /// Backend-agnostic LRU texture cache. Calls `backend.upload_image` /
    /// `evict_image` for actual GPU work.
    texture_cache: TextureCache,
    /// Scratch plan filled by resolve, passed to `backend.present_frame`.
    frame_plan: FramePlan,
    /// Frame counter used as LRU timestamp.
    frame_counter: u64,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
    /// The physical size the backend's surface was last given, so a
    /// configure that changes nothing in pixels leaves it alone.
    configured: (u32, u32),
    /// Whether the shortfall has already been reported, so a frame that keeps
    /// dropping the same draw says so once instead of every frame.
    warned_about_dropped: bool,
}

impl<B: Gpu2dBackend> Gpu2dRenderer<B> {
    /// Wrap a constructed backend. `width`/`height` are logical; `scale_factor`
    /// maps to physical pixels (matching [`CpuRasterizer`](crate::cpu::CpuRasterizer)).
    pub fn from_backend(backend: B, width: u32, height: u32, scale_factor: f32) -> Self {
        Self {
            backend,
            display_list: DisplayList::new(),
            batches: RenderBatches::default(),
            texture_cache: TextureCache::new(),
            frame_plan: FramePlan::new(),
            frame_counter: 0,
            warned_about_dropped: false,
            logical_width: width,
            logical_height: height,
            scale_factor: scale_factor.max(1.0),
            // The backend is built at this size.
            configured: physical(width, height, scale_factor.max(1.0)),
        }
    }

    /// Whether the target has no size, so nothing can be drawn.
    fn suspended(&self) -> bool {
        self.logical_width == 0 || self.logical_height == 0
    }

    /// Shared access to the device backend (for backend-specific extensions
    /// such as engine-side image embedding via `draw_sampled_image`).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable access to the device backend.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Logical size scaled to physical pixels (clamped to ≥ 1).
    fn physical_size(&self) -> (u32, u32) {
        physical(self.logical_width, self.logical_height, self.scale_factor)
    }

    /// Says once that this backend is leaving draws out of the frame.
    ///
    /// Reported rather than ignored because the window looks plausible: the
    /// missing draws leave no gap, no error and nothing in the frame to
    /// suggest the CPU rasterizer would have shown more.
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

impl<B: Gpu2dBackend + Send + Sync> Renderer for Gpu2dRenderer<B> {
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
        // A scale change alone changes the pixel size too; the surface has
        // to follow either.
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
        self.report_dropped();
        let (pw, ph) = self.physical_size();
        self.frame_plan.viewport_width = pw;
        self.frame_plan.viewport_height = ph;
        self.frame_counter += 1;
        let fc = self.frame_counter;
        resolve_frame(
            &self.batches,
            &mut self.texture_cache,
            &mut self.backend,
            fc,
            &mut self.frame_plan,
        )?;
        let rects = &self.batches.rects;
        let circles = &self.batches.circles;
        self.backend.present_frame(&self.frame_plan, rects, circles)
    }

    fn cleanup(&mut self) {
        self.display_list.clear();
    }

    fn set_damage(&mut self, _damage: Option<Rect>) {}

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
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
    use crate::batch::{CircleInstance, RectInstance};

    /// Records what the renderer asks of it; presents with `outcome`.
    struct Probe {
        resized: Vec<(u32, u32)>,
        outcome: FrameOutcome,
    }

    impl Gpu2dBackend for Probe {
        fn resize(&mut self, w: u32, h: u32) -> AureaResult<()> {
            self.resized.push((w, h));
            Ok(())
        }

        fn upload_image(&mut self, _: u32, _: u32, _: &[u8]) -> AureaResult<u32> {
            Ok(0)
        }

        fn evict_image(&mut self, _: u32) {}

        fn present_frame(
            &mut self,
            _: &FramePlan,
            _: &[RectInstance],
            _: &[CircleInstance],
        ) -> AureaResult<FrameOutcome> {
            Ok(self.outcome)
        }
    }

    fn renderer(outcome: FrameOutcome) -> Gpu2dRenderer<Probe> {
        let probe = Probe {
            resized: Vec::new(),
            outcome,
        };
        Gpu2dRenderer::from_backend(probe, 100, 50, 1.0)
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
        // Nothing changed in pixels: the surface is left alone.
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
    fn a_frame_the_surface_refused_is_reported() {
        let mut r = renderer(FrameOutcome::Retry);
        drop(r.begin_frame().expect("begin"));
        let outcome = r.end_frame().expect("end");
        assert_eq!(outcome, FrameOutcome::Retry);
        assert!(outcome.needs_redraw());
    }
}
