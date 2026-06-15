//! Shared ZenGPU context bootstrap for an Aurea editor and game viewport.
//!
//! This proves the ownership contract required before editor composition:
//! Aurea's 2D renderer and engine-side offscreen resources use one logical
//! Vulkan device. Rendering the game target inside the editor is the next step.

use std::sync::Arc;

use aurea::render::ZenGpuContext;
use aurea::Window;
use zengpu_vulkan::{vk, OffscreenTarget};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("Aurea - shared ZenGPU context", 1280, 720)?;
    let context = Arc::new(ZenGpuContext::new()?);

    let editor_renderer = window.create_zengpu_2d_with_context(Arc::clone(&context))?;
    let game_view = OffscreenTarget::new(
        &context.device_context(),
        vk::Format::R8G8B8A8_UNORM,
        960,
        540,
    )?;

    println!(
        "editor={}x{}, game_view={}x{}, one shared Vulkan device",
        editor_renderer.size().0,
        editor_renderer.size().1,
        game_view.extent().width,
        game_view.extent().height,
    );

    Ok(())
}
