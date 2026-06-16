//! The single backend seam for the shared GPU-2D core ([`super::Gpu2dRenderer`]).

use aurea_foundation::AureaResult;

use super::frame_plan::FramePlan;

/// Device-touching primitives driven by [`super::Gpu2dRenderer`] each frame.
pub trait Gpu2dBackend {
    /// (Re)create the swapchain/surface for a new **physical** pixel size.
    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()>;

    /// Per-frame setup before resolve.
    fn begin_frame(&mut self) -> AureaResult<()> {
        Ok(())
    }

    /// Upload RGBA8 pixel data to the GPU and return a shader-visible slot index for use in the
    /// fragment shader's bindless texture array.
    fn upload_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> AureaResult<u32>;

    /// Release the GPU texture at `shader_slot` (previously returned by
    /// [`upload_image`](Self::upload_image)).
    fn evict_image(&mut self, shader_slot: u32);

    /// Whether the backend supports a dual-source-blend text path (coverage composited per RGB
    /// channel).
    fn supports_dual_source(&self) -> bool {
        false
    }

    /// Upload instance arrays from `plan`, record painter-order draws, and present one frame.
    fn present_frame(
        &mut self,
        plan: &FramePlan,
        rects: &[crate::batch::RectInstance],
        circles: &[crate::batch::CircleInstance],
    ) -> AureaResult<()>;
}
