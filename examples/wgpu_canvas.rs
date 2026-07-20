//! WGPU surface from a canvas.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    use aurea::elements::{Box, BoxOrientation};
    use aurea::render::{Canvas, RendererBackend};
    use aurea::{Container, Window};

    let mut window = Window::new("WGPU Canvas", 800, 600)?;
    let canvas = Canvas::new(800, 600, RendererBackend::Cpu)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let _surface = canvas.create_wgpu_surface(&instance)?;

    let mut layout = Box::new(BoxOrientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.run()?;
    Ok(())
}
