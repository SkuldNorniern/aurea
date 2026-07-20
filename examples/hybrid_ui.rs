//! Hybrid UI Demo: Native Widgets + Custom Canvas.

use aurea::elements::{Stack, Orientation, Button, Label};
use aurea::render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend};
use aurea::{AureaResult, Container, Window};

const CANVAS_WIDTH: u32 = 600;
const CANVAS_HEIGHT: u32 = 400;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Hybrid UI Demo - Native + Canvas", 800, 600)?;

    let mut canvas = Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT, RendererBackend::Cpu)?;

    canvas.draw(|ctx| {
        ctx.clear(Color::rgb(245, 245, 250))?;

        let color = Color::rgb(100, 150, 200);
        let paint_fill = Paint::new().color(color).style(PaintStyle::Fill);

        let paint_stroke = Paint::new()
            .color(color)
            .style(PaintStyle::Stroke)
            .stroke_width(3.0);

        ctx.draw_rect(Rect::new(50.0, 50.0, 120.0, 80.0), &paint_fill)?;
        ctx.draw_rect(Rect::new(200.0, 50.0, 120.0, 80.0), &paint_stroke)?;

        ctx.draw_circle(Point::new(150.0, 200.0), 50.0, &paint_fill)?;
        ctx.draw_circle(Point::new(300.0, 200.0), 50.0, &paint_stroke)?;

        ctx.draw_rect(Rect::new(400.0, 150.0, 100.0, 100.0), &paint_fill)?;

        let border_paint = Paint::new()
            .color(Color::rgb(200, 200, 200))
            .style(PaintStyle::Stroke)
            .stroke_width(2.0);
        ctx.draw_rect(
            Rect::new(0.0, 0.0, CANVAS_WIDTH as f32, CANVAS_HEIGHT as f32),
            &border_paint,
        )?;

        Ok(())
    })?;

    let mut control_panel = Stack::new(Orientation::Vertical)?;

    control_panel.add(Label::new("Canvas Controls")?)?;

    control_panel.add(Button::new("Shapes Mode")?)?;
    control_panel.add(Button::new("Grid Mode")?)?;
    control_panel.add(Button::new("Circles Mode")?)?;
    control_panel.add(Button::new("Waves Mode")?)?;
    control_panel.add(Button::new("Change Color")?)?;

    control_panel.add(Label::new("Mode: Shapes | Color: Blue")?)?;

    control_panel.add(Label::new("")?)?;
    control_panel.add(Label::new("This demo shows:")?)?;
    control_panel.add(Label::new("• Native buttons")?)?;
    control_panel.add(Label::new("• Native labels")?)?;
    control_panel.add(Label::new("• Custom canvas")?)?;
    control_panel.add(Label::new("• All in one window")?)?;

    let mut main_layout = Stack::new(Orientation::Horizontal)?;
    main_layout.add(control_panel)?;
    main_layout.add(canvas)?;

    window.set_content(main_layout)?;

    window.run()?;

    Ok(())
}
