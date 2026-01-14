//! Hybrid UI Demo: Native Widgets + Custom Canvas
//!
//! This example demonstrates Aurea's unique capability to seamlessly
//! combine native UI elements (buttons, labels) with custom-drawn canvas content.
//!
//! Features:
//! - Native buttons and labels in a control panel
//! - Custom canvas with different drawing modes
//! - Demonstrates retained-mode, event-driven architecture
//! - Shows how native widgets and canvas can coexist in the same window

use aurea::elements::{Box, BoxOrientation, Button, Label};
use aurea::render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend};
use aurea::{AureaResult, Container, Window};

const CANVAS_WIDTH: u32 = 600;
const CANVAS_HEIGHT: u32 = 400;

fn main() -> AureaResult<()> {
    // Create window
    let mut window = Window::new("Hybrid UI Demo - Native + Canvas", 800, 600)?;

    // Create canvas with CPU rasterizer
    let mut canvas = Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT, RendererBackend::Cpu)?;

    // Draw initial canvas content (Shapes mode)
    canvas.draw(|ctx| {
        // Clear with light background
        ctx.clear(Color::rgb(245, 245, 250))?;

        let color = Color::rgb(100, 150, 200); // Blue
        let paint_fill = Paint::new().color(color).style(PaintStyle::Fill);

        let paint_stroke = Paint::new()
            .color(color)
            .style(PaintStyle::Stroke)
            .stroke_width(3.0);

        // Draw various shapes
        ctx.draw_rect(Rect::new(50.0, 50.0, 120.0, 80.0), &paint_fill)?;
        ctx.draw_rect(Rect::new(200.0, 50.0, 120.0, 80.0), &paint_stroke)?;

        ctx.draw_circle(Point::new(150.0, 200.0), 50.0, &paint_fill)?;
        ctx.draw_circle(Point::new(300.0, 200.0), 50.0, &paint_stroke)?;

        // Animated rectangle (static for demo)
        ctx.draw_rect(Rect::new(400.0, 150.0, 100.0, 100.0), &paint_fill)?;

        // Draw border
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

    // Create control panel with native widgets
    let mut control_panel = Box::new(BoxOrientation::Vertical)?;

    // Title label
    control_panel.add(Label::new("Canvas Controls")?)?;

    // Drawing mode buttons
    // Note: In a full implementation, these would update canvas content
    // For this demo, they show the button functionality
    control_panel.add(Button::new("Shapes Mode")?)?;
    control_panel.add(Button::new("Grid Mode")?)?;
    control_panel.add(Button::new("Circles Mode")?)?;
    control_panel.add(Button::new("Waves Mode")?)?;
    control_panel.add(Button::new("Change Color")?)?;

    // Status label
    control_panel.add(Label::new("Mode: Shapes | Color: Blue")?)?;

    // Info label
    control_panel.add(Label::new("")?)?; // Spacer
    control_panel.add(Label::new("This demo shows:")?)?;
    control_panel.add(Label::new("• Native buttons")?)?;
    control_panel.add(Label::new("• Native labels")?)?;
    control_panel.add(Label::new("• Custom canvas")?)?;
    control_panel.add(Label::new("• All in one window")?)?;

    // Create main layout: horizontal box with controls and canvas
    let mut main_layout = Box::new(BoxOrientation::Horizontal)?;
    main_layout.add(control_panel)?;
    main_layout.add(canvas)?;

    // Set window content
    window.set_content(main_layout)?;

    // Run event loop
    window.run()?;

    Ok(())
}
