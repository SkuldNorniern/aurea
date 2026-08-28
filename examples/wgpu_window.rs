//! WGPU surface from a window.

use aurea::Window;
use std::error::Error;
use wgpu::{Instance, InstanceDescriptor};

fn main() -> Result<(), Box<dyn Error>> {
    let window = Window::new("WGPU Window", 800, 600)?;
    let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
    let _surface = window.create_wgpu_surface(&instance)?;
    window.show();
    window.run()?;
    Ok(())
}
