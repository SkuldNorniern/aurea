//! Shared ZenGPU context bootstrap for an Aurea editor and game viewport.

use aurea::render::{Rect, ZenGpuContext};
use aurea::Window;
use std::sync::Arc;
use zengpu::{vulkan::vk, OffscreenTarget};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("Aurea - shared ZenGPU context", 1280, 720)?;
    let context = Arc::new(ZenGpuContext::new()?);

    let mut host_renderer = window.create_zengpu_2d_with_context(Arc::clone(&context))?;
    let game_view = OffscreenTarget::new(
        &context.device_context(),
        vk::Format::R8G8B8A8_UNORM,
        960,
        540,
    )?;
    host_renderer.draw_sampled_image(
        game_view.sampled_view(),
        Rect::new(32.0, 32.0, 960.0, 540.0),
    )?;

    println!(
        "host={}x{}, embedded_view={}x{}, bound on one Vulkan device without CPU readback",
        host_renderer.size().0,
        host_renderer.size().1,
        game_view.extent().width,
        game_view.extent().height,
    );

    host_renderer.clear_sampled_images()?;
    Ok(())
}
