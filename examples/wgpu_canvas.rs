//! WGPU surface from a canvas.

#[cfg(feature = "wgpu")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use aurea::elements::{Box, BoxOrientation};
    use aurea::render::{Canvas, RendererBackend};
    use aurea::{Container, Window};

    let mut window = Window::new("WGPU Canvas", 800, 600)?;
    let canvas = Canvas::new(800, 600, RendererBackend::Cpu)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let _surface = canvas.create_wgpu_surface(&instance)?;

    let mut layout = Box::new(BoxOrientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.run()?;
    Ok(())
}

#[cfg(not(feature = "wgpu"))]
fn main() {
    eprintln!("Build with --features wgpu to run this example.");
    std::process::exit(1);
}
