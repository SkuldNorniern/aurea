//! WGPU surface from a window.

use std::error::Error;
#[cfg(not(feature = "wgpu"))]
use std::process::exit;

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(not(feature = "wgpu"))]
    {
        eprintln!("This example requires the `wgpu` feature.");
        eprintln!("Run with: cargo run --example wgpu_window --features wgpu");
        exit(1);
    }

    #[cfg(feature = "wgpu")]
    {
        let window = aurea::Window::new("WGPU Window", 800, 600)?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let _surface = window.create_wgpu_surface(&instance)?;
        window.show();
        window.run()?;
        Ok(())
    }
}
