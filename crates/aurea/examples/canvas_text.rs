//! Canvas text API example. See **fontkit** example for the main font kit demo.
//!
//! Demonstrates: draw_text, draw_text_with_font, measure_text (Skia/Vello-style).

use aurea::elements::{Box, BoxOrientation};
use aurea::render::{
    Canvas, Color, Font, FontStyle, FontWeight, Paint, PaintStyle, Point, Rect, RendererBackend,
};
use aurea::{AureaResult, Container, Window};

const W: u32 = 720;
const H: u32 = 480;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Canvas Text", W as i32, H as i32)?;
    let mut canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(252, 250, 248));
    canvas.draw(|ctx| draw_text_showcase(ctx))?;

    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    main_box.add(canvas)?;
    window.set_content(main_box)?;
    window.run()?;

    Ok(())
}

fn draw_text_showcase(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let black = Paint::new()
        .color(Color::rgb(20, 20, 20))
        .style(PaintStyle::Fill);
    let gray = Paint::new()
        .color(Color::rgb(100, 100, 100))
        .style(PaintStyle::Fill);
    let blue = Paint::new()
        .color(Color::rgb(40, 80, 160))
        .style(PaintStyle::Fill);

    ctx.draw_text(
        "Default font (draw_text)",
        Point::new(24.0, 40.0),
        &black,
    )?;

    let font_sans = Font::new("", 24.0);
    ctx.draw_text_with_font(
        "System sans 24px",
        Point::new(24.0, 80.0),
        &font_sans,
        &black,
    )?;

    let font_large = Font::new("", 36.0);
    ctx.draw_text_with_font(
        "Large 36px",
        Point::new(24.0, 130.0),
        &font_large,
        &blue,
    )?;

    let font_bold = Font::new("", 20.0).with_weight(FontWeight::Bold);
    ctx.draw_text_with_font(
        "Bold weight",
        Point::new(24.0, 180.0),
        &font_bold,
        &black,
    )?;

    let font_italic = Font::new("", 20.0).with_style(FontStyle::Italic);
    ctx.draw_text_with_font(
        "Italic style",
        Point::new(24.0, 210.0),
        &font_italic,
        &black,
    )?;

    let font_serif = Font::new("serif", 22.0);
    ctx.draw_text_with_font(
        "Serif family 22px",
        Point::new(24.0, 250.0),
        &font_serif,
        &gray,
    )?;

    let title = "Centered with measure_text";
    let measure_font = Font::new("", 28.0);
    let metrics = ctx.measure_text(title, &measure_font)?;
    let box_rect = Rect::new(24.0, 290.0, 400.0, 50.0);
    let stroke = Paint::new()
        .color(Color::rgb(200, 200, 200))
        .style(PaintStyle::Stroke)
        .stroke_width(1.0);
    ctx.draw_rect(box_rect, &stroke)?;
    let x = box_rect.x + (box_rect.width - metrics.width) / 2.0;
    let y = box_rect.y + box_rect.height / 2.0 + metrics.ascent / 2.0 - metrics.descent / 2.0;
    ctx.draw_text_with_font(title, Point::new(x, y), &measure_font, &black)?;

    let line2 = "Skia/Vello-style: Font, measure_text, draw_text";
    let small = Font::new("", 14.0);
    let m2 = ctx.measure_text(line2, &small)?;
    let box2 = Rect::new(24.0, 360.0, 500.0, 32.0);
    ctx.draw_rect(box2, &stroke)?;
    let x2 = box2.x + (box2.width - m2.width) / 2.0;
    let y2 = box2.y + box2.height / 2.0 + m2.ascent / 2.0 - m2.descent / 2.0;
    ctx.draw_text_with_font(line2, Point::new(x2, y2), &small, &gray)?;

    Ok(())
}
