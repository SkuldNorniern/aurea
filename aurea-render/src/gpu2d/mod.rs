//! Backend-agnostic GPU-2D rendering core.

mod backend;
mod frame_plan;
mod resolve;
mod texture_cache;

pub use backend::Gpu2dBackend;
pub use frame_plan::FramePlan;

use aurea_foundation::AureaResult;

use crate::batch::RenderBatches;
use crate::cpu::CpuDrawingContext;
use crate::display_list::DisplayList;
use crate::renderer::{DrawingContext, Renderer};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::Rect;

use resolve::resolve_frame;
use texture_cache::TextureCache;

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
            logical_width: width,
            logical_height: height,
            scale_factor: scale_factor.max(1.0),
        }
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
        let s = self.scale_factor;
        (
            ((self.logical_width as f32 * s).round() as u32).max(1),
            ((self.logical_height as f32 * s).round() as u32).max(1),
        )
    }
}

impl<B: Gpu2dBackend + Send + Sync> Renderer for Gpu2dRenderer<B> {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.logical_width = info.width;
        self.logical_height = info.height;
        self.scale_factor = info.scale_factor.max(1.0);
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.logical_width = width;
        self.logical_height = height;
        self.display_list.clear();
        let (pw, ph) = self.physical_size();
        self.backend.resize(pw, ph)
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        self.backend.begin_frame()?;
        self.texture_cache.prune_dropped(&mut self.backend);
        self.display_list.clear();
        let mut ctx = CpuDrawingContext::new(
            &mut self.display_list as *mut DisplayList,
            self.logical_width,
            self.logical_height,
        );
        ctx.set_scale_factor(self.scale_factor);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        self.batches.lower_into(&self.display_list);
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
