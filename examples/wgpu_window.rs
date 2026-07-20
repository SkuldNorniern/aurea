//! WGPU surface from a window.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let window = aurea::Window::new("WGPU Window", 800, 600)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let _surface = window.create_wgpu_surface(&instance)?;
    window.show();
    window.run()?;
    Ok(())
}
