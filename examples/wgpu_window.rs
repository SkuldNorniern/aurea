//! WGPU surface from a window.
//!
//! Creates a window and a wgpu surface from it. Run with: cargo run --example wgpu_window --features wgpu
//!
//! In a render loop, on `Surface::get_current_texture()` error use
//! `aurea::integration::wgpu::handle_surface_error_for_window()`; after recreating the surface
//! call `notify_surface_recreated_for_window()`. See `aurea::integration::wgpu` docs.

use aurea::Window;
use std::error::Error;
use wgpu::{Instance, InstanceDescriptor};

fn main() -> Result<(), Box<dyn Error>> {
    let window = Window::new("WGPU Window", 800, 600)?;
    let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
    // The surface borrows the window, so the window has to outlive it — which
    // is the real constraint the platform imposes.
    let _surface = window.create_wgpu_surface(&instance)?;
    window.show();
    window.run()?;
    Ok(())
}
