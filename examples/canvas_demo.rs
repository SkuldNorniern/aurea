//! Canvas demo showcasing CPU rasterizer with tile-based rendering
//!
//! This example demonstrates:
//! - Basic drawing operations (rectangles, circles)
//! - Tile-based rendering with partial redraw support
//! - CPU-first rasterization approach

use aurea::{Window, AureaResult, Container};
use aurea::render::{Canvas, Color, Paint, PaintStyle, Rect, Point, RendererBackend};
use aurea::elements::{Box, BoxOrientation};

const CANVAS_WIDTH: u32 = 800;
const CANVAS_HEIGHT: u32 = 600;

fn main() -> AureaResult<()> {
    // Create window (platform initializes automatically)
    let mut window = Window::new("Canvas Demo - CPU Rasterizer", CANVAS_WIDTH as i32, CANVAS_HEIGHT as i32)?;
    
    // Create canvas with CPU rasterizer backend
    let mut canvas = Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT, RendererBackend::Cpu)?;
    
    // Draw shapes on the canvas
    canvas.draw(|ctx| {
        draw_scene(ctx)
    })?;
    
    // Create layout and add canvas
    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    main_box.add(canvas)?;
    
    // Set window content and run
    window.set_content(main_box)?;
    window.run()?;
    
    Ok(())
}

/// Draw the demo scene
fn draw_scene(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    // Clear with light gray background
    ctx.clear(Color::rgb(240, 240, 240))?;
    
    // Draw filled rectangle
    let blue_fill = Paint::new()
        .color(Color::rgb(100, 150, 200))
        .style(PaintStyle::Fill);
    ctx.draw_rect(Rect::new(50.0, 50.0, 200.0, 150.0), &blue_fill)?;
    
    // Draw stroked rectangle
    let red_stroke = Paint::new()
        .color(Color::rgb(200, 100, 100))
        .style(PaintStyle::Stroke)
        .stroke_width(3.0);
    ctx.draw_rect(Rect::new(300.0, 50.0, 200.0, 150.0), &red_stroke)?;
    
    // Draw filled circle
    let green_fill = Paint::new()
        .color(Color::rgb(100, 200, 100))
        .style(PaintStyle::Fill);
    ctx.draw_circle(Point::new(150.0, 300.0), 50.0, &green_fill)?;
    
    // Draw stroked circle
    let orange_stroke = Paint::new()
        .color(Color::rgb(200, 150, 100))
        .style(PaintStyle::Stroke)
        .stroke_width(4.0);
    ctx.draw_circle(Point::new(400.0, 300.0), 60.0, &orange_stroke)?;
    
    // Draw overlapping shapes (tests compositing)
    let semi_transparent = Paint::new()
        .color(Color::rgba(150, 150, 255, 180))
        .style(PaintStyle::Fill);
    ctx.draw_rect(Rect::new(100.0, 400.0, 300.0, 100.0), &semi_transparent)?;
    
    let yellow_fill = Paint::new()
        .color(Color::rgb(255, 200, 100))
        .style(PaintStyle::Fill);
    ctx.draw_circle(Point::new(250.0, 450.0), 40.0, &yellow_fill)?;
    
    // Draw canvas border
    let border = Paint::new()
        .color(Color::rgb(50, 50, 50))
        .style(PaintStyle::Stroke)
        .stroke_width(2.0);
    ctx.draw_rect(Rect::new(0.0, 0.0, CANVAS_WIDTH as f32, CANVAS_HEIGHT as f32), &border)?;
    
    Ok(())
}

