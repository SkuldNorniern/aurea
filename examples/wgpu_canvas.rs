//! WGPU surface from a canvas.

use aurea::elements::{Orientation, Stack};
use aurea::render::{Canvas, RendererBackend};
use aurea::{Container, Window};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut window = Window::new("WGPU Canvas", 800, 600)?;
    let canvas = Canvas::new(800, 600, RendererBackend::Cpu)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let _surface = canvas.create_wgpu_surface(&instance)?;

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.run()?;
    Ok(())
}
