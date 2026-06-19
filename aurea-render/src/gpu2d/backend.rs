//! The single backend seam for the shared GPU-2D core ([`super::Gpu2dRenderer`]).
//!
//! A `Gpu2dBackend` owns only device + surface bring-up and the per-frame draw
//! primitives. Texture-cache policy, slot assignment, and display-list resolution
//! live in the core (`gpu2d/texture_cache.rs`, `gpu2d/resolve.rs`). This mirrors
//! `cpu/`'s split and ZenGPU's split-HAL rule: *add a backend without
//! re-duplicating the layer above it*.

use aurea_foundation::AureaResult;

use crate::batch::{CircleInstance, RectInstance};

use super::frame_plan::FramePlan;

/// Device-touching primitives driven by [`super::Gpu2dRenderer`] each frame.
///
/// Calling order per frame:
/// 1. [`begin_frame`](Self::begin_frame) — per-frame setup.
/// 2. (core resolves textures via [`upload_image`](Self::upload_image) /
///    [`evict_image`](Self::evict_image), building a [`FramePlan`])
/// 3. [`present_frame`](Self::present_frame) — GPU upload + record + present.
///
/// Resize can happen any time between frames via [`resize`](Self::resize).
pub trait Gpu2dBackend {
    /// (Re)create the swapchain/surface for a new **physical** pixel size.
    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()>;

    /// Per-frame setup before resolve. Default: nothing.
    fn begin_frame(&mut self) -> AureaResult<()> {
        Ok(())
    }

    /// Upload RGBA8 pixel data to the GPU and return a shader-visible slot
    /// index for use in the fragment shader's bindless texture array.
    ///
    /// The slot is stable until [`evict_image`](Self::evict_image) is called
    /// with the same value. The backend is responsible for any internal
    /// slot→resource mapping.
    fn upload_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> AureaResult<u32>;

    /// Release the GPU texture at `shader_slot` (previously returned by
    /// [`upload_image`](Self::upload_image)).
    fn evict_image(&mut self, shader_slot: u32);

    /// Whether the backend supports a dual-source-blend text path (coverage
    /// composited per RGB channel). Default: `false` (portable alpha fallback).
    fn supports_dual_source(&self) -> bool {
        false
    }

    /// Upload instance arrays from `plan`, record painter-order draws, and
    /// present one frame. Called after the core resolves all textures.
    fn present_frame(
        &mut self,
        plan: &FramePlan,
        rects: &[RectInstance],
        circles: &[CircleInstance],
    ) -> AureaResult<()>;
}
