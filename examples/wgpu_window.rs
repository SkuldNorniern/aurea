//! WGPU surface from a window.

#[cfg(feature = "wgpu")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = aurea::Window::new("WGPU Window", 800, 600)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let _surface = window.create_wgpu_surface(&instance)?;
    window.show();
    window.run()?;
    Ok(())
}

#[cfg(not(feature = "wgpu"))]
fn main() {
    eprintln!("Build with --features wgpu to run this example.");
    std::process::exit(1);
}
