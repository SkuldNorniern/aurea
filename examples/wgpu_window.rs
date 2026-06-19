//! WGPU surface from a window.

use std::error::Error;
#[cfg(not(feature = "wgpu"))]
use std::process::exit;

#[cfg(feature = "wgpu")]
fn main() -> Result<(), Box<dyn Error>> {
    let window = aurea::Window::new("WGPU Window", 800, 600)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let _surface = window.create_wgpu_surface(&instance)?;
    window.show();
    window.run()?;
    Ok(())
}

#[cfg(not(feature = "wgpu"))]
fn main() {
    eprintln!("Build with --features wgpu to run this example.");
    exit(1);
}
