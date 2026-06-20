//! Surface bring-up for the unified-API painter — `device.create_surface()` only.

use zengpu_hal::{Format, GraphicsDevice, PresentMode, Result, SurfaceConfig, WindowHandles};
use zengpu_vulkan::{VulkanDevice, VulkanSurface};

/// Create the painter's presentable surface.
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
            present_mode: PresentMode::Mailbox,
        },
    )
}
