//! The single backend seam for the shared GPU-2D core ([`super::Gpu2dRenderer`]).
//!
//! A `Gpu2dBackend` owns only device + surface bring-up and the per-frame draw
//! primitives. Everything above it — the display list, batch lowering, and (as
//! of P8-B) the texture/slot cache and painter-order record loop — is
//! backend-independent and lives in the core. This mirrors `cpu/`'s split and
//! ZenGPU's own split-HAL rule: *add a backend without re-duplicating the layer
//! above it*.

use aurea_foundation::AureaResult;

use crate::batch::RenderBatches;

/// Device-touching primitives a [`super::Gpu2dRenderer`] drives each frame.
///
/// The core calls these in `Renderer` order: [`begin_frame`](Self::begin_frame)
/// at the start of recording, [`present`](Self::present) once the display list
/// has been lowered to [`RenderBatches`], and [`resize`](Self::resize) on
/// surface size changes. A backend resolves textures, uploads instance streams,
/// records, and presents inside `present` — the core stays free of `vk`/`wgpu`.
pub trait Gpu2dBackend {
    /// (Re)create the swapchain/surface for a new **physical** pixel size.
    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()>;

    /// Per-frame setup, called at the start of `begin_frame` before recording.
    /// Default: nothing. (ZenGPU releases caller-owned external image slots here.)
    fn begin_frame(&mut self) -> AureaResult<()> {
        Ok(())
    }

    /// Resolve/upload textures, record the painter-order draws, and present one
    /// frame described by `batches` (already lowered from the display list).
    fn present(&mut self, batches: &RenderBatches) -> AureaResult<()>;
}
