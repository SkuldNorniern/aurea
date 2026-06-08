//! Font kit example: canvas text API (Skia/Vello-style).
//!
//! Demonstrates the font kit via canvas: draw_text (default font),
//! draw_text_with_font (family, size, weight, style), measure_text for
//! layout and centering.

use aurea::elements::{Box, BoxOrientation};
use aurea::render::{
    Canvas, Color, Font, FontStyle, FontWeight, Paint, PaintStyle, Point, Rect, RendererBackend,
};
use aurea::{AureaResult, Container, Window};

const W: u32 = 720;
const H: u32 = 560;
const TOP_MARGIN: f32 = 0.0;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Font Kit", W as i32, H as i32)?;
    let mut canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(252, 250, 248));
    canvas.set_draw_callback(|ctx| draw_fontkit(ctx))?;
    canvas.draw(|ctx| draw_fontkit(ctx))?;

    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    main_box.add_weighted(canvas, 1.0)?;
    window.set_content(main_box)?;
    window.run()?;

    Ok(())
}

fn draw_fontkit(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let black = Paint::new()
        .color(Color::rgb(30, 30, 30))
        .style(PaintStyle::Fill);
    let gray = Paint::new()
        .color(Color::rgb(100, 100, 100))
        .style(PaintStyle::Fill);
    let blue = Paint::new()
        .color(Color::rgb(40, 80, 160))
        .style(PaintStyle::Fill);

    ctx.draw_text(
        "Default font (draw_text)",
        Point::new(24.0, TOP_MARGIN + 32.0),
        &black,
    )?;

    let font_sans = Font::new("", 18.0);
    ctx.draw_text_with_font(
        "System sans 18px",
        Point::new(24.0, TOP_MARGIN + 56.0),
        &font_sans,
        &black,
    )?;

    let font_large = Font::new("", 24.0);
    ctx.draw_text_with_font(
        "Large 24px",
        Point::new(24.0, TOP_MARGIN + 88.0),
        &font_large,
        &blue,
    )?;

    let font_bold = Font::new("", 16.0).with_weight(FontWeight::Bold);
    ctx.draw_text_with_font(
        "Bold weight",
        Point::new(24.0, TOP_MARGIN + 120.0),
        &font_bold,
        &black,
    )?;

    let font_italic = Font::new("", 16.0).with_style(FontStyle::Italic);
    ctx.draw_text_with_font(
        "Italic style",
        Point::new(24.0, TOP_MARGIN + 144.0),
        &font_italic,
        &black,
    )?;

    let font_serif = Font::new("serif", 16.0);
    ctx.draw_text_with_font(
        "Serif family 16px",
        Point::new(24.0, TOP_MARGIN + 168.0),
        &font_serif,
        &gray,
    )?;

    let title = "Centered with measure_text";
    let measure_font = Font::new("", 20.0);
    let metrics = ctx.measure_text(title, &measure_font)?;
    let box_rect = Rect::new(24.0, TOP_MARGIN + 200.0, 400.0, 40.0);
    let stroke = Paint::new()
        .color(Color::rgb(200, 200, 200))
        .style(PaintStyle::Stroke)
        .stroke_width(1.0);
    ctx.draw_rect(box_rect, &stroke)?;
    let x = box_rect.x + (box_rect.width - metrics.width) / 2.0;
    let y = box_rect.y + box_rect.height / 2.0 + metrics.ascent / 2.0 - metrics.descent / 2.0;
    ctx.draw_text_with_font(title, Point::new(x, y), &measure_font, &black)?;

    let line2 = "Font kit: Font, measure_text, draw_text via canvas";
    let small = Font::new("", 12.0);
    let m2 = ctx.measure_text(line2, &small)?;
    let box2 = Rect::new(24.0, TOP_MARGIN + 260.0, 500.0, 28.0);
    ctx.draw_rect(box2, &stroke)?;
    let x2 = box2.x + (box2.width - m2.width) / 2.0;
    let y2 = box2.y + box2.height / 2.0 + m2.ascent / 2.0 - m2.descent / 2.0;
    ctx.draw_text_with_font(line2, Point::new(x2, y2), &small, &gray)?;

    Ok(())
}
