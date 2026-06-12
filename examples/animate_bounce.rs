//! Bouncing ball animation example.
//!
//! A ball loops back and forth across the canvas forever, accelerating and
//! decelerating smoothly at each end using `InOutQuad` easing.
//!
//! Demonstrates:
//! - Looping `Animation` (wraps at the end instead of stopping)
//! - `EaseMode::InOutQuad`
//! - Early cancellation: pressing any key cancels the animation via `TickerId`
//! - Storing the `TickerId` for later cancellation

use std::sync::{Arc, Mutex};
use std::time::Duration;

use aurea::elements::{Box, BoxOrientation};
use aurea::render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend};
use aurea::{AureaResult, Container, FrameScheduler, Window};
use aurea_animation::{Animation, EaseMode};

const W: u32 = 700;
const H: u32 = 300;
const BALL_R: f32 = 30.0;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Animate — Bouncing Ball (looping)", W as i32, H as i32)?;

    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(20, 20, 35));

    // Shared x-position for the ball (in pixels, left edge of travel range).
    let ball_x: Arc<Mutex<f32>> = Arc::new(Mutex::new(BALL_R));
    let ball_x_draw = ball_x.clone();
    let ball_x_tick = ball_x.clone();

    canvas.set_draw_callback(move |ctx| {
        let x = *ball_x_draw.lock().unwrap();
        let cy = H as f32 / 2.0;

        // Track line
        let track = Paint::new()
            .color(Color::rgba(255, 255, 255, 30))
            .style(PaintStyle::Stroke)
            .stroke_width(1.0);
        ctx.draw_rect(Rect::new(BALL_R, cy - 1.0, W as f32 - BALL_R * 2.0, 2.0), &track)?;

        // Shadow
        let shadow = Paint::new()
            .color(Color::rgba(0, 0, 0, 80))
            .style(PaintStyle::Fill);
        ctx.draw_circle(Point::new(x, cy + BALL_R + 6.0), BALL_R * 0.5, &shadow)?;

        // Ball
        let ball = Paint::new()
            .color(Color::rgb(80, 180, 255))
            .style(PaintStyle::Fill);
        ctx.draw_circle(Point::new(x, cy), BALL_R, &ball)?;

        // Highlight
        let hi = Paint::new()
            .color(Color::rgba(255, 255, 255, 140))
            .style(PaintStyle::Fill);
        ctx.draw_circle(Point::new(x - BALL_R * 0.3, cy - BALL_R * 0.3), BALL_R * 0.25, &hi)?;

        let label = Paint::new()
            .color(Color::rgba(180, 180, 180, 180))
            .style(PaintStyle::Fill);
        ctx.draw_text("InOutQuad looping — any key cancels", Point::new(10.0, 20.0), &label)?;

        Ok(())
    })?;

    // Looping 2 s animation.  The ball travels from x = BALL_R to x = W - BALL_R.
    let travel = W as f32 - BALL_R * 2.0;
    let mut anim = Animation::new(Duration::from_millis(2000))
        .ease(EaseMode::InOutQuad)
        .looping(true);

    let ticker_id = canvas.animate(move |info| {
        let t = match anim.tick(info.delta) {
            Some(v) => v,
            None => return false,
        };
        // Ping-pong: go right on even laps, left on odd laps.
        // We implement ping-pong by mirroring t every other half-period.
        // Since Animation already loops at t=1 → t=0, we do a simple triangle:
        let mirrored = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
        *ball_x_tick.lock().unwrap() = BALL_R + mirrored * travel;
        true
    });

    // Register a key handler that cancels the animation.
    window.on_event(move |event| {
        if let aurea::WindowEvent::KeyInput { .. } = event {
            FrameScheduler::unregister_ticker(ticker_id);
        }
    });

    let mut layout = Box::new(BoxOrientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.run()?;

    Ok(())
}
