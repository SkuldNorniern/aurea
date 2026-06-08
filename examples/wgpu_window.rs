//! WGPU surface from a window.
//!
//! Creates a window and a wgpu surface from it. Run with: cargo run --example wgpu_window --features wgpu
//!
//! In a render loop, on `Surface::get_current_texture()` error use
//! `aurea::integration::wgpu::handle_surface_error_for_window()`; after recreating the surface
//! call `notify_surface_recreated_for_window()`. See `aurea::integration::wgpu` docs.

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
