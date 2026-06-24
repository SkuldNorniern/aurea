use aurea::AureaResult;
#[cfg(feature = "zengpu")]
use aurea::{
    Container, Window, WindowEvent,
    elements::{Box as NativeBox, BoxOrientation},
    render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend},
};
#[cfg(not(feature = "zengpu"))]
use std::process::exit;

fn main() -> AureaResult<()> {
    #[cfg(not(feature = "zengpu"))]
    {
        eprintln!("This example requires the `zengpu` feature.");
        eprintln!("Run with: cargo run --example zengpu_demo --features zengpu");
        exit(1);
    }

    #[cfg(feature = "zengpu")]
    {
        const W: u32 = 800;
        const H: u32 = 600;

        let mut window = Window::new("Canvas Demo - ZenGPU", W as i32, H as i32)?;
        let canvas = Canvas::new(W, H, RendererBackend::ZenGpu)?;

        canvas.set_draw_callback(|ctx| {
            ctx.clear(Color::rgb(240, 240, 240))?;

            ctx.draw_rect(
                Rect::new(50.0, 50.0, 200.0, 150.0),
                &Paint::new()
                    .color(Color::rgb(100, 150, 200))
                    .style(PaintStyle::Fill),
            )?;

            ctx.draw_rect(
                Rect::new(300.0, 50.0, 200.0, 150.0),
                &Paint::new()
                    .color(Color::rgb(200, 100, 100))
                    .style(PaintStyle::Stroke)
                    .stroke_width(3.0),
            )?;

            ctx.draw_circle(
                Point::new(150.0, 350.0),
                80.0,
                &Paint::new()
                    .color(Color::rgb(100, 200, 100))
                    .style(PaintStyle::Fill),
            )?;

            ctx.draw_circle(
                Point::new(400.0, 350.0),
                80.0,
                &Paint::new()
                    .color(Color::rgb(200, 150, 100))
                    .style(PaintStyle::Stroke)
                    .stroke_width(4.0),
            )?;

            ctx.draw_rect(
                Rect::new(100.0, 470.0, 300.0, 80.0),
                &Paint::new()
                    .color(Color::rgba(150, 150, 255, 180))
                    .style(PaintStyle::Fill),
            )?;

            ctx.draw_circle(
                Point::new(250.0, 510.0),
                40.0,
                &Paint::new()
                    .color(Color::rgb(255, 200, 100))
                    .style(PaintStyle::Fill),
            )?;

            Ok(())
        })?;

        let mut layout = NativeBox::new(BoxOrientation::Vertical)?;
        layout.add(canvas)?;
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
