use aurea::AureaResult;
#[cfg(feature = "zengpu")]
use aurea::{
    Container, Window, WindowEvent,
    elements::{Box as NativeBox, BoxOrientation, Label},
    render::{Canvas, Color, Paint, Point, Rect, RendererBackend},
};
#[cfg(not(feature = "zengpu"))]
use std::process::exit;

fn main() -> AureaResult<()> {
    #[cfg(not(feature = "zengpu"))]
    {
        eprintln!("This example requires the `zengpu` feature.");
        eprintln!("Run with: cargo run --example zengpu_canvas --features zengpu");
        exit(1);
    }

    #[cfg(feature = "zengpu")]
    {
        let mut window = Window::new("Aurea - hosted ZenGPU canvas", 800, 600)?;
        let canvas = Canvas::new(800, 600, RendererBackend::ZenGpu)?;

        // Register before attachment to exercise deferred swapchain creation.
        canvas.set_draw_callback(|ctx| {
            ctx.clear(Color::rgb(22, 24, 32))?;
            ctx.draw_rect(
                Rect::new(80.0, 80.0, 300.0, 180.0),
                &Paint::new().color(Color::rgb(85, 150, 255)),
            )?;
            ctx.draw_circle(
                Point::new(420.0, 300.0),
                110.0,
                &Paint::new().color(Color::rgba(255, 110, 160, 220)),
            )?;
            ctx.draw_rect(
                Rect::new(390.0, 270.0, 180.0, 90.0),
                &Paint::new().color(Color::rgb(40, 42, 54)),
            )?;
            Ok(())
        })?;

        let mut layout = NativeBox::new(BoxOrientation::Vertical)?;
        layout.add(Label::new(
            "Native label above a compositor-hosted ZenGPU canvas",
        )?)?;
        layout.add_weighted(canvas, 1.0)?;
        window.set_content(layout)?;
        loop {
            for event in window.poll_events() {
                if matches!(event, WindowEvent::CloseRequested) {
                    return Ok(());
                }
            }
            window.process_frames()?;
        }
    }
}
