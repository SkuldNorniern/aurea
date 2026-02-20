//! Canvas image demo: draw_image, draw_image_rect, draw_image_region.
//!
//! Creates a small RGBA image and draws it at position, scaled to a rect,
//! and as a region (crop) to verify the image drawing path.

use aurea::elements::{Box, BoxOrientation};
use aurea::render::{Canvas, Color, Image, Paint, PaintStyle, Point, Rect, RendererBackend};
use aurea::{AureaResult, Container, Window};

const CANVAS_WIDTH: u32 = 640;
const CANVAS_HEIGHT: u32 = 480;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Canvas Image Demo", CANVAS_WIDTH as i32, CANVAS_HEIGHT as i32)?;
    let mut canvas = Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(248, 248, 248));

    canvas.draw(|ctx| draw_image_scene(ctx))?;

    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    main_box.add(canvas)?;
    window.set_content(main_box)?;
    window.run()?;

    Ok(())
}

fn draw_image_scene(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let image = make_test_image();

    ctx.draw_image(&image, Point::new(20.0, 20.0))?;

    ctx.draw_image_rect(
        &image,
        Rect::new(220.0, 20.0, 120.0, 120.0),
    )?;

    ctx.draw_image_region(
        &image,
        Rect::new(10.0, 10.0, 30.0, 30.0),
        Rect::new(360.0, 20.0, 90.0, 90.0),
    )?;

    let border = Paint::new()
        .color(Color::rgb(180, 180, 180))
        .style(PaintStyle::Stroke)
        .stroke_width(1.0);
    ctx.draw_rect(Rect::new(20.0, 20.0, 80.0, 80.0), &border)?;
    ctx.draw_rect(Rect::new(220.0, 20.0, 120.0, 120.0), &border)?;
    ctx.draw_rect(Rect::new(360.0, 20.0, 90.0, 90.0), &border)?;

    Ok(())
}

fn make_test_image() -> Image {
    const W: u32 = 80;
    const H: u32 = 80;
    let mut data = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) as usize * 4;
            data[i] = (x * 3) as u8;
            data[i + 1] = (y * 3) as u8;
            data[i + 2] = 180;
            data[i + 3] = 255;
        }
    }
    Image::new(W, H, data)
}
