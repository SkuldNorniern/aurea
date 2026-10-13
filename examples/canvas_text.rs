//! Canvas text API example.

use aurea::elements::{Orientation, Stack};
use aurea::render::{
    Canvas, Color, DrawingContext, Font, FontStyle, FontWeight, LayoutOptions, Paint, PaintStyle,
    Point, Rect, RendererBackend, TextAlign, WordBreak, layout_text,
};
use aurea::{AureaResult, Container, Window};

const W: u32 = 720;
const H: u32 = 480;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Canvas Text", W as i32, H as i32)?;
    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(252, 250, 248));
    canvas.set_draw_callback(draw_text_showcase)?;

    let mut main_box = Stack::new(Orientation::Vertical)?;
    main_box.add(canvas)?;
    window.set_content(main_box)?;
    window.run()?;

    Ok(())
}

fn draw_text_showcase(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
    let black = Paint::new()
        .color(Color::rgb(20, 20, 20))
        .style(PaintStyle::Fill);
    let gray = Paint::new()
        .color(Color::rgb(100, 100, 100))
        .style(PaintStyle::Fill);
    let blue = Paint::new()
        .color(Color::rgb(40, 80, 160))
        .style(PaintStyle::Fill);

    ctx.draw_text("Default font (draw_text)", Point::new(24.0, 40.0), &black)?;

    let font_sans = Font::new("", 24.0);
    ctx.draw_text_with_font(
        "System sans 24px",
        Point::new(24.0, 80.0),
        &font_sans,
        &black,
    )?;

    let font_large = Font::new("", 36.0);
    ctx.draw_text_with_font("Large 36px", Point::new(24.0, 130.0), &font_large, &blue)?;

    let font_bold = Font::new("", 20.0).with_weight(FontWeight::Bold);
    ctx.draw_text_with_font("Bold weight", Point::new(24.0, 180.0), &font_bold, &black)?;

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

    let shaped = Font::new("", 20.0);
    let samples = [
        "Hello 안녕하세요 世界",
        "مرحبا بالعالم",
        "cafe\u{301} re\u{301}sume\u{301}",
        "abc עברית 123",
    ];
    for (i, sample) in samples.iter().enumerate() {
        let y = 60.0 + i as f32 * 40.0;
        ctx.draw_text_with_font(sample, Point::new(420.0, y), &shaped, &blue)?;
    }
    let ligatures = Font::new("Calibri", 22.0);
    ctx.draw_text_with_font(
        "office affine fjord",
        Point::new(420.0, 230.0),
        &ligatures,
        &black,
    )?;
    let apart = ligatures.clone().with_feature("liga", 0);
    ctx.draw_text_with_font("office (liga off)", Point::new(420.0, 258.0), &apart, &gray)?;

    let body = Font::new("", 14.0);
    let paragraph = "Text wraps at word boundaries, 줄바꿈도 됩니다, and keeps \
                     right-to-left words like שלום in order.";
    let options = LayoutOptions {
        max_width: Some(230.0),
        align: TextAlign::Center,
        line_height: 1.2,
        word_break: WordBreak::KeepAll,
    };
    let layout = layout_text(paragraph, &body, options)?;
    let frame = Rect::new(436.0, 280.0, 238.0, layout.height + 8.0);
    ctx.draw_rect(frame, &stroke)?;
    ctx.draw_text_layout(&layout, Point::new(440.0, 284.0), &body, &black)?;

    let column = Font::new("", 20.0).with_vertical(true);
    let columns = LayoutOptions {
        max_width: Some(180.0),
        ..LayoutOptions::default()
    };
    let vertical = layout_text("縦書きの文字は右から左へ並びます", &column, columns)?;
    let at = Point::new(704.0 - vertical.width, 40.0);
    ctx.draw_text_layout(&vertical, at, &column, &blue)?;

    for (i, weight) in [300.0, 500.0, 700.0].into_iter().enumerate() {
        let face = Font::new("Bahnschrift", 20.0).with_variation("wght", weight);
        let x = 24.0 + i as f32 * 130.0;
        ctx.draw_text_with_font("Variable", Point::new(x, 450.0), &face, &black)?;
    }

    Ok(())
}
