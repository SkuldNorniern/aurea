//! Bouncing ball animation example.
//!
//! A ball loops back and forth across the canvas forever, accelerating and
//! decelerating smoothly at each end using `InOutQuad` easing.
//!
//! Demonstrates:
//! - Looping `Animation` (wraps at the end instead of stopping)
//! - `EaseMode::InOutQuad`
//! - Early cancellation: pressing any key freezes the ball
//! - The poll-loop render pattern: `poll_events` → `draw` → `process_frames`

use std::time::{Duration, Instant};

use aurea::render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend};
use aurea::{AureaResult, Window, WindowEvent};
use aurea_animation::{Animation, EaseMode};

const W: u32 = 700;
const H: u32 = 300;
const BALL_R: f32 = 30.0;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Animate — Bouncing Ball (looping)", W as i32, H as i32)?;

    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(20, 20, 35));

    let mut draw_canvas = canvas.clone();
    window.set_content(canvas)?;

    // Looping 2 s animation.  The ball travels from x = BALL_R to x = W - BALL_R.
    let travel = W as f32 - BALL_R * 2.0;
    let mut anim = Animation::new(Duration::from_millis(2000))
        .ease(EaseMode::InOutQuad)
        .looping(true);

    let mut ball_x: f32 = BALL_R;
    let mut running = true;
    let mut last = Instant::now();

    'main: loop {
        for event in window.poll_events() {
            match event {
                WindowEvent::CloseRequested => break 'main,
                // Any key cancels the animation; the ball freezes in place.
                WindowEvent::KeyInput { .. } => running = false,
                _ => {}
            }
        }

        let now = Instant::now();
        let delta = now - last;
        last = now;

        if running {
            if let Some(t) = anim.tick(delta) {
                // Ping-pong: mirror t every other half-period so the ball goes
                // right for the first half and left for the second.
                let mirrored = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
                ball_x = BALL_R + mirrored * travel;
            }
        }

        draw_canvas.draw(|ctx| {
            let cy = H as f32 / 2.0;

            // Track line
            let track = Paint::new()
                .color(Color::rgba(255, 255, 255, 30))
                .style(PaintStyle::Stroke)
                .stroke_width(1.0);
            ctx.draw_rect(
                Rect::new(BALL_R, cy - 1.0, W as f32 - BALL_R * 2.0, 2.0),
                &track,
            )?;

            // Shadow
            let shadow = Paint::new()
                .color(Color::rgba(0, 0, 0, 80))
                .style(PaintStyle::Fill);
            ctx.draw_circle(Point::new(ball_x, cy + BALL_R + 6.0), BALL_R * 0.5, &shadow)?;

            // Ball
            let ball = Paint::new()
                .color(Color::rgb(80, 180, 255))
                .style(PaintStyle::Fill);
            ctx.draw_circle(Point::new(ball_x, cy), BALL_R, &ball)?;

            // Highlight
            let hi = Paint::new()
                .color(Color::rgba(255, 255, 255, 140))
                .style(PaintStyle::Fill);
            ctx.draw_circle(
                Point::new(ball_x - BALL_R * 0.3, cy - BALL_R * 0.3),
                BALL_R * 0.25,
                &hi,
            )?;

            let label = Paint::new()
                .color(Color::rgba(180, 180, 180, 180))
                .style(PaintStyle::Fill);
            ctx.draw_text(
                "InOutQuad looping — any key cancels",
                Point::new(10.0, 20.0),
                &label,
            )?;

            Ok(())
        })?;

        window.process_frames()?;
        std::thread::sleep(Duration::from_millis(8));
    }

    Ok(())
}
