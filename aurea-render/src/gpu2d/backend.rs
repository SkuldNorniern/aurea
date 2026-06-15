//! The single backend seam for the shared GPU-2D core ([`super::Gpu2dRenderer`]).

use aurea_foundation::AureaResult;

use crate::batch::RenderBatches;

/// Device-touching primitives a [`super::Gpu2dRenderer`] drives each frame.
pub trait Gpu2dBackend {
    /// (Re)create the swapchain/surface for a new **physical** pixel size.
    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()>;

    /// Per-frame setup, called at the start of `begin_frame` before recording.
    fn begin_frame(&mut self) -> AureaResult<()> {
        Ok(())
    }

    /// Resolve/upload textures, record the painter-order draws, and present one frame described by
    /// `batches` (already lowered from the display list).
    fn present(&mut self, batches: &RenderBatches) -> AureaResult<()>;
}
