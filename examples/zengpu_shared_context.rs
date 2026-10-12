//! Shared ZenGPU context bootstrap for an Aurea editor and game viewport.

use aurea::{
    Window,
    render::{GpuTexture, Rect, Renderer, ZenGpuContext},
};
use std::{error::Error, sync::Arc};
use zengpu::{Format, vulkan::OffscreenTarget};

fn main() -> Result<(), Box<dyn Error>> {
    let window = Window::new("Aurea - shared ZenGPU context", 1280, 720)?;
    let context = Arc::new(ZenGpuContext::new()?);

    let mut host_renderer = window.create_zengpu_2d_with_context(Arc::clone(&context))?;
    let game_view = OffscreenTarget::new(context.device(), Format::Rgba8Unorm, 960, 540)?;
    let (gw, gh) = game_view.extent();
    let texture = GpuTexture::from_zengpu(game_view.texture_handle(), gw, gh);

    let mut ctx = host_renderer.begin_frame()?;
    ctx.draw_texture(
        texture,
        Rect::new(0.0, 0.0, 1.0, 1.0),
        Rect::new(32.0, 32.0, 960.0, 540.0),
    )?;
    drop(ctx);
    host_renderer.end_frame()?;

    println!(
        "host={}x{}, embedded_view={gw}x{gh}, drawn on one Vulkan device without CPU readback",
        host_renderer.size().0,
        host_renderer.size().1,
    );
    Ok(())
}
