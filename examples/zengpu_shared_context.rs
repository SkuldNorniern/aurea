//! Shared ZenGPU context bootstrap for an Aurea editor and game viewport.

#[cfg(feature = "zengpu")]
use aurea::Window;
#[cfg(feature = "zengpu")]
use aurea::render::{Rect, ZenGpuContext};
use std::error::Error;
#[cfg(not(feature = "zengpu"))]
use std::process::exit;
#[cfg(feature = "zengpu")]
use std::sync::Arc;
#[cfg(feature = "zengpu")]
use zengpu::{Format, OffscreenTarget};

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(not(feature = "zengpu"))]
    {
        eprintln!("This example requires the `zengpu` feature.");
        eprintln!("Run with: cargo run --example zengpu_shared_context --features zengpu");
        exit(1);
    }

    #[cfg(feature = "zengpu")]
    {
        let window = Window::new("Aurea - shared ZenGPU context", 1280, 720)?;
        let context = Arc::new(ZenGpuContext::new()?);

        let mut host_renderer = window.create_zengpu_2d_with_context(Arc::clone(&context))?;
        let game_view = OffscreenTarget::new(context.device(), Format::Rgba8Unorm, 960, 540)?;
        host_renderer.draw_sampled_image(
            game_view.texture_handle(),
            Rect::new(32.0, 32.0, 960.0, 540.0),
        )?;

        let (gw, gh) = game_view.extent();
        println!(
            "host={}x{}, embedded_view={}x{}, bound on one Vulkan device without CPU readback",
            host_renderer.size().0,
            host_renderer.size().1,
            gw,
            gh,
        );

        host_renderer.clear_sampled_images()?;
        Ok(())
    }
}
