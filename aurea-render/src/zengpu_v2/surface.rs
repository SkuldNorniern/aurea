//! Surface bring-up for the unified-API painter — `device.create_surface()`
//! only. The returned [`VulkanSurface`] owns its swapchain and rebuilds it
//! internally on resize/loss (`acquire`/`present`/`resize`/`size` from
//! [`zengpu_hal::Surface`]); no raw `vk::SwapchainKHR`/sync objects here.

use zengpu_hal::{Format, GraphicsDevice, PresentMode, Result, SurfaceConfig, WindowHandles};
use zengpu_vulkan::{VulkanDevice, VulkanSurface};

/// Create the painter's presentable surface. `width`/`height` are physical pixels.
pub fn create_surface(
    device: &VulkanDevice,
    window: &WindowHandles,
    width: u32,
    height: u32,
) -> Result<VulkanSurface> {
    device.create_surface(
        window,
        SurfaceConfig {
            format: Format::Bgra8Unorm,
            width: width.max(1),
            height: height.max(1),
            present_mode: PresentMode::Fifo,
        },
    )
}
