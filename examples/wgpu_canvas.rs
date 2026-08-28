//! WGPU surface from a canvas.
//!
//! Creates a window with a canvas and a wgpu surface from the canvas. Run with: cargo run --example wgpu_canvas --features wgpu
//!
//! In a render loop, on `Surface::get_current_texture()` error use
//! `aurea::integration::wgpu::handle_surface_error_for_canvas()`; after recreating the surface
//! call `notify_surface_recreated_for_canvas()`. See `aurea::integration::wgpu` docs.

use aurea::elements::{Orientation, Stack};
use aurea::render::{Canvas, RendererBackend};
use aurea::{Container, Window};
use std::error::Error;
use wgpu::{Instance, InstanceDescriptor};

fn main() -> Result<(), Box<dyn Error>> {
    let mut window = Window::new("WGPU Canvas", 800, 600)?;
    let canvas = Canvas::new(800, 600, RendererBackend::Cpu)?;
    let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
    // Canvas clones share one native canvas, so keep a clone for the surface to
    // borrow while the other goes into the layout.
    let surface_canvas = canvas.clone();
    let _surface = surface_canvas.create_wgpu_surface(&instance)?;

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.run()?;
    Ok(())
}
