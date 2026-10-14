//! The single backend seam for the shared GPU-2D core ([`super::Gpu2dRenderer`]).

use aurea_foundation::{AureaError, AureaResult};

use crate::capabilities::GpuFeatures;
use crate::gpu::batch::{DrawRef, RectInstance, ShapeInstance};
use crate::renderer::FrameOutcome;
use crate::types::GpuTexture;

use super::frame_plan::{FramePlan, ImagePlanEntry};

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

    /// Like [`upload_image`](Self::upload_image), with a mip chain made on the device, for an image
    /// drawn with [`Sampling::Smooth`] smaller than it is.
    ///
    /// [`Sampling::Smooth`]: crate::Sampling::Smooth
    fn upload_image_mipmapped(&mut self, width: u32, height: u32, rgba: &[u8]) -> AureaResult<u32> {
        self.upload_image(width, height, rgba)
    }

    /// Release the GPU texture at `shader_slot` (previously returned by
    /// [`upload_image`](Self::upload_image)).
    fn evict_image(&mut self, shader_slot: u32);

    /// The shader slot an app-made texture draws from this frame.
    fn bind_texture(&mut self, texture: GpuTexture) -> AureaResult<u32> {
        let _ = texture;
        Err(AureaError::RenderingFailed)
    }

    /// The kinds of drawing this backend does itself.
    fn features(&self) -> GpuFeatures;

    /// Presents `rgba` (tightly packed RGBA8, `width` x `height` physical pixels) as the whole
    /// frame: how a frame drawn on the CPU reaches this backend's surface.
    fn present_pixels(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> AureaResult<FrameOutcome> {
        let slot = self.upload_image(width, height, rgba)?;
        let mut plan = FramePlan::new();
        plan.viewport_width = width;
        plan.viewport_height = height;
        #[allow(clippy::cast_precision_loss)]
        plan.images.push(ImagePlanEntry {
            affine: [0.0; 4],
            rect: [0.0, 0.0, width as f32, height as f32],
            uv: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0; 4],
            sampling: [width as f32, height as f32, 0.0, 0.0],
            slot,
        });
        plan.order.push(DrawRef::Image(0));
        plan.clips.push(None);
        let outcome = self.present_frame(&plan, &[], &[]);
        self.evict_image(slot);
        outcome
    }

    /// Whether the backend supports a dual-source-blend text path (coverage composited per RGB
    /// channel).
    fn supports_dual_source(&self) -> bool {
        false
    }

    /// Upload instance arrays from `plan`, record painter-order draws, and present one frame.
    fn present_frame(
        &mut self,
        plan: &FramePlan,
        rects: &[RectInstance],
        shapes: &[ShapeInstance],
    ) -> AureaResult<FrameOutcome>;
}
