//! Demonstrates blend modes on the CPU canvas.
//!
//! Draws background rects, then overlapping shapes with Multiply, Screen,
//! Overlay, Difference, Darken, and Lighten so you can see how each mode
//! combines with the content underneath.

use aurea::elements::{Box, BoxOrientation};
use aurea::render::{
    BlendMode, Canvas, Color, Paint, Point, Rect, RendererBackend,
};
use aurea::{AureaResult, Container, Window};

const CANVAS_WIDTH: u32 = 640;
const CANVAS_HEIGHT: u32 = 480;

fn main() -> AureaResult<()> {
    let mut window =
        Window::new("Canvas Blend Demo", CANVAS_WIDTH as i32, CANVAS_HEIGHT as i32)?;
    let mut canvas = Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(240, 240, 240));

    canvas.draw(|ctx| draw_blend_scene(ctx))?;

    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    main_box.add(canvas)?;
    window.set_content(main_box)?;
    window.run()?;

    Ok(())
}

fn draw_blend_scene(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let base = Paint::new().color(Color::rgb(200, 180, 160));
    ctx.draw_rect(Rect::new(20.0, 20.0, 280.0, 200.0), &base)?;

    ctx.set_blend_mode(BlendMode::Multiply)?;
    ctx.draw_rect(
        Rect::new(40.0, 40.0, 120.0, 80.0),
        &Paint::new().color(Color::rgba(255, 100, 100, 200)),
    )?;
    ctx.draw_rect(
        Rect::new(120.0, 80.0, 120.0, 80.0),
        &Paint::new().color(Color::rgba(100, 200, 100, 200)),
    )?;

    ctx.set_blend_mode(BlendMode::Screen)?;
    ctx.draw_circle(
        Point::new(200.0, 140.0),
        50.0,
        &Paint::new().color(Color::rgba(100, 100, 255, 180)),
    )?;

    ctx.set_blend_mode(BlendMode::Normal)?;
    let base2 = Paint::new().color(Color::rgb(100, 120, 140));
    ctx.draw_rect(Rect::new(340.0, 20.0, 280.0, 200.0), &base2)?;

    ctx.set_blend_mode(BlendMode::Overlay)?;
    ctx.draw_rect(
        Rect::new(360.0, 40.0, 100.0, 80.0),
        &Paint::new().color(Color::rgba(255, 200, 80, 220)),
    )?;

    ctx.set_blend_mode(BlendMode::Difference)?;
    ctx.draw_circle(
        Point::new(480.0, 120.0),
        55.0,
        &Paint::new().color(Color::rgba(200, 150, 255, 200)),
    )?;

    ctx.set_blend_mode(BlendMode::Darken)?;
    ctx.draw_rect(
        Rect::new(20.0, 250.0, 180.0, 100.0),
        &Paint::new().color(Color::rgb(180, 100, 100)),
    )?;
    ctx.draw_rect(
        Rect::new(80.0, 290.0, 180.0, 100.0),
        &Paint::new().color(Color::rgb(100, 100, 180)),
    )?;

    ctx.set_blend_mode(BlendMode::Lighten)?;
    ctx.draw_rect(
        Rect::new(340.0, 250.0, 180.0, 100.0),
        &Paint::new().color(Color::rgb(255, 180, 100)),
    )?;
    ctx.draw_rect(
        Rect::new(400.0, 290.0, 180.0, 100.0),
        &Paint::new().color(Color::rgb(100, 200, 255)),
    )?;

    ctx.set_blend_mode(BlendMode::Normal)?;
    Ok(())
}
