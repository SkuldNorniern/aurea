//! ZenGPU triangle rendered into an aurea window.
//!
//! Opens a native window via aurea, then wires it to ZenGPU's Vulkan
//! backend.  The triangle shader lives entirely inside `zengpu-vulkan`
//! (compiled from GLSL at build time); this file only drives the event
//! and render loop.
//!
//! Run with:
//!   cargo run --example zengpu_triangle
//!
//! Requires Vulkan 1.2+ and a `VK_KHR_swapchain`-capable driver.

use aurea::{Window, WindowEvent};
use zengpu_hal::{
    DeviceRequest, Format, GpuAdapter, GpuInstance, PresentMode, SurfaceConfig, WindowHandles,
};
use zengpu_vulkan::instance::VulkanInstance;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("ZenGPU — Triangle", 800, 600)?;

    let inst = VulkanInstance::new_with_surface()?;

    let adapter = inst
        .request_vulkan_adapter()
        .ok_or("no Vulkan adapter found")?;
    eprintln!("ZenGPU: {}", adapter.info().name);

    let device = adapter.open_with_surface(DeviceRequest::default())?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| format!("window handle unavailable: {e:?}"))?;
    let (w, h) = window.size();
    let config = SurfaceConfig {
        format: Format::Bgra8Unorm,
        width: w,
        height: h,
        present_mode: PresentMode::Fifo,
    };

    let surface = inst.create_surface(&handles, &device, config)?;

    'main: loop {
        for event in window.poll_events() {
            if matches!(event, WindowEvent::CloseRequested) {
                break 'main;
            }
        }

        let frame = surface.acquire_frame()?;
        surface.present_frame(frame)?;
    }

    Ok(())
}
