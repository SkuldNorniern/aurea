//! Fade-in animation example.
//!
//! A colored panel fades from transparent to opaque over 1.5 seconds using an
//! OutCubic easing curve.  After the animation finishes, the canvas stays at
//! full opacity.
//!
//! Demonstrates:
//! - One-shot `Animation` ticked from the application loop
//! - `EaseMode::OutCubic` (slow-down at end)
//! - The poll-loop render pattern: `poll_events` → `draw` → `process_frames`
//! - `Canvas::clone` — one clone is the window content, the other draws

use std::thread::sleep;
use std::time::{Duration, Instant};

use aurea::render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend};
use aurea::{AureaResult, Window, WindowEvent};
use aurea_animation::{Animation, EaseMode};

const W: u32 = 640;
const H: u32 = 480;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Animate — Fade In", W as i32, H as i32)?;

    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(30, 30, 30));

    // One clone becomes the window content; this one stays for drawing.
    let mut draw_canvas = canvas.clone();
    window.set_content(canvas)?;

    // One-shot animation: 1.5 s, OutCubic.
    let mut anim = Animation::new(Duration::from_millis(1500)).ease(EaseMode::OutCubic);
    let mut alpha: f32 = 0.0;
    let mut last = Instant::now();

    'main: loop {
        for event in window.poll_events() {
            if matches!(event, WindowEvent::CloseRequested) {
                break 'main;
            }
        }

        let now = Instant::now();
        let delta = now - last;
        last = now;

        if let Some(t) = anim.tick(delta) {
            alpha = t;
        }

        draw_canvas.draw(|ctx| {
            // Background label
            let label_paint = Paint::new()
                .color(Color::rgb(180, 180, 180))
                .style(PaintStyle::Fill);
            ctx.draw_text(
                "OutCubic fade-in (1.5 s)",
                Point::new(20.0, 30.0),
                &label_paint,
            )?;

            // The panel: alpha goes from 0 → 255 as t goes 0 → 1.
            let a = (alpha * 255.0) as u8;
            let panel = Paint::new()
                .color(Color::rgba(80, 140, 220, a))
                .style(PaintStyle::Fill);
            ctx.draw_rect(Rect::new(100.0, 80.0, 440.0, 300.0), &panel)?;

            // Thin white border so the panel is visible even at low alpha.
            let border = Paint::new()
                .color(Color::rgba(255, 255, 255, 60))
                .style(PaintStyle::Stroke)
                .stroke_width(1.5);
            ctx.draw_rect(Rect::new(100.0, 80.0, 440.0, 300.0), &border)?;

            // Progress text
            let pct = (alpha * 100.0) as u32;
            let pct_str = format!("{}%", pct);
            let pct_paint = Paint::new()
                .color(Color::rgb(220, 220, 220))
                .style(PaintStyle::Fill);
            ctx.draw_text(&pct_str, Point::new(20.0, H as f32 - 20.0), &pct_paint)?;

            Ok(())
        })?;

        window.process_frames()?;
        sleep(Duration::from_millis(8));
    }

    Ok(())
}
