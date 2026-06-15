//! Backend-agnostic GPU-2D rendering core.
//!
//! [`Gpu2dRenderer`] implements aurea's [`Renderer`] trait **once**, over any
//! [`Gpu2dBackend`]. It owns the display list, batch lowering, and logical/
//! physical size bookkeeping; the backend supplies only device-touching
//! primitives. ZenGPU is the first backend on this core (the showcase for
//! ZenGPU's unified, no-raw-`vk` graphics API); wgpu follows on the same seam,
//! so the cache/record machinery exists once instead of once per backend.
//!
//! This is the GPU peer of the modular `cpu/` backend: one concern per piece,
//! a thin device boundary, and "add a backend = implement one trait".

mod backend;

pub use backend::Gpu2dBackend;

use aurea_foundation::AureaResult;

use crate::batch::RenderBatches;
use crate::cpu::CpuDrawingContext;
use crate::display_list::DisplayList;
use crate::renderer::{DrawingContext, Renderer};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::Rect;

/// A 2D GPU renderer parameterized over its device backend.
///
/// Records draw calls into a [`DisplayList`] (the same [`CpuDrawingContext`] the
/// CPU rasterizer uses), lowers them to [`RenderBatches`] in `end_frame`, and
/// hands the batches to the backend to resolve, record, and present. Holds no
/// `vk`/`wgpu` types.
pub struct Gpu2dRenderer<B: Gpu2dBackend> {
    backend: B,
    display_list: DisplayList,
    /// Reused across frames so steady-state `end_frame` allocates nothing.
    batches: RenderBatches,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
}

impl<B: Gpu2dBackend> Gpu2dRenderer<B> {
    /// Wrap a constructed backend. `width`/`height` are logical; `scale_factor`
    /// maps to physical pixels (matching [`CpuRasterizer`](crate::cpu::CpuRasterizer)).
    pub fn from_backend(backend: B, width: u32, height: u32, scale_factor: f32) -> Self {
        Self {
            backend,
            display_list: DisplayList::new(),
            batches: RenderBatches::default(),
            logical_width: width,
            logical_height: height,
            scale_factor: scale_factor.max(1.0),
        }
    }

    /// Shared access to the device backend (for backend-specific extensions such
    /// as engine-side image embedding).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable access to the device backend.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Logical size scaled to physical pixels (clamped to ≥ 1).
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
        self.backend.present(&self.batches)
    }

    fn cleanup(&mut self) {
        self.display_list.clear();
    }

    fn set_damage(&mut self, _damage: Option<Rect>) {
        // The GPU painter redraws the full frame each present; damage is unused.
    }

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }
}
