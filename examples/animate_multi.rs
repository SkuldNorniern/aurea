//! Multiple simultaneous animations example.
//!
//! Three independent animations run at the same time on the same canvas:
//! - A square slides in from the left  (OutQuint, 1.2 s)
//! - A circle fades + scales in        (OutCubic, 0.8 s, delayed 0.4 s)
//! - A progress bar fills up           (InOutQuad, 2.0 s, looping)
//!
//! Demonstrates:
//! - Multiple concurrent tickers on one canvas
//! - Manual animation delay by skipping ticks until a timer elapses
//! - Composing the draw callback from shared state written by separate tickers

use std::sync::{Arc, Mutex};
use std::time::Duration;

use aurea::elements::{Box, BoxOrientation};
use aurea::render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend};
use aurea::{AureaResult, Container, Window};
use aurea_animation::{Animation, EaseMode};

const W: u32 = 700;
const H: u32 = 500;

/// All animated values in one struct so the draw callback takes one clone.
#[derive(Clone)]
struct Scene {
    slide_x: f32,   // square left edge
    circle_alpha: u8,
    circle_scale: f32,
    bar_fill: f32,  // 0..1
}

fn main() -> AureaResult<()> {
    let mut window = Window::new("Animate — Multiple Simultaneous", W as i32, H as i32)?;

    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(245, 245, 248));

    let scene = Arc::new(Mutex::new(Scene {
        slide_x: -(200.0_f32), // starts off-screen
        circle_alpha: 0,
        circle_scale: 0.0,
        bar_fill: 0.0,
    }));

    // — Draw callback --------------------------------------------------------
    let scene_draw = scene.clone();
    canvas.set_draw_callback(move |ctx| {
        let s = scene_draw.lock().unwrap().clone();

        // ── Sliding square ────────────────────────────────────────────────
        let sq = Paint::new()
            .color(Color::rgb(80, 120, 200))
            .style(PaintStyle::Fill);
        ctx.draw_rect(Rect::new(s.slide_x, 60.0, 200.0, 200.0), &sq)?;
        let sq_label = Paint::new()
            .color(Color::rgb(255, 255, 255))
            .style(PaintStyle::Fill);
        ctx.draw_text("Slide (OutQuint)", Point::new(s.slide_x + 10.0, 165.0), &sq_label)?;

        // ── Fading + scaling circle ───────────────────────────────────────
        let cx = W as f32 * 0.65;
        let cy = 165.0_f32;
        let r = 80.0 * s.circle_scale;
        if r > 0.5 {
            let circle = Paint::new()
                .color(Color::rgba(200, 80, 80, s.circle_alpha))
                .style(PaintStyle::Fill);
            ctx.draw_circle(Point::new(cx, cy), r, &circle)?;
            let ci_label = Paint::new()
                .color(Color::rgba(255, 255, 255, s.circle_alpha))
                .style(PaintStyle::Fill);
            ctx.draw_text(
                "Fade+Scale (OutCubic)",
                Point::new(cx - 75.0, cy + 5.0),
                &ci_label,
            )?;
        }

        // ── Progress bar ──────────────────────────────────────────────────
        let bar_y = H as f32 - 80.0;
        let bar_x = 50.0_f32;
        let bar_w = W as f32 - 100.0;
        let bar_h = 24.0_f32;

        let bar_bg = Paint::new()
            .color(Color::rgb(210, 210, 215))
            .style(PaintStyle::Fill);
        ctx.draw_rect(Rect::new(bar_x, bar_y, bar_w, bar_h), &bar_bg)?;

        let bar_fill = Paint::new()
            .color(Color::rgb(60, 180, 110))
            .style(PaintStyle::Fill);
        ctx.draw_rect(Rect::new(bar_x, bar_y, bar_w * s.bar_fill, bar_h), &bar_fill)?;

        let border = Paint::new()
            .color(Color::rgb(160, 160, 165))
            .style(PaintStyle::Stroke)
            .stroke_width(1.0);
        ctx.draw_rect(Rect::new(bar_x, bar_y, bar_w, bar_h), &border)?;

        let pct = format!("{}% (InOutQuad, looping)", (s.bar_fill * 100.0) as u32);
        let bar_label = Paint::new()
            .color(Color::rgb(60, 60, 60))
            .style(PaintStyle::Fill);
        ctx.draw_text(&pct, Point::new(bar_x, bar_y - 18.0), &bar_label)?;

        Ok(())
    })?;

    // — Ticker 1: sliding square (OutQuint, 1.2 s) ---------------------------
    {
        let scene_t1 = scene.clone();
        let start_x = -(200.0_f32);
        let end_x = 40.0_f32;
        let mut anim = Animation::new(Duration::from_millis(1200)).ease(EaseMode::OutQuint);

        canvas.animate(move |info| {
            match anim.tick(info.delta) {
                Some(t) => {
                    scene_t1.lock().unwrap().slide_x = start_x + (end_x - start_x) * t;
                    true
                }
                None => {
                    // Park at final position.
                    scene_t1.lock().unwrap().slide_x = end_x;
                    false
                }
            }
        });
    }

    // — Ticker 2: fading circle (OutCubic, 0.8 s, with 0.4 s initial delay) --
    {
        let scene_t2 = scene.clone();
        let delay = Duration::from_millis(400);
        let mut waited = Duration::ZERO;
        let mut anim = Animation::new(Duration::from_millis(800)).ease(EaseMode::OutCubic);
        let mut started = false;

        canvas.animate(move |info| {
            // Delay phase: just count time, don't start the animation yet.
            if !started {
                waited += info.delta;
                if waited < delay { return true; }
                started = true;
            }

            match anim.tick(info.delta) {
                Some(t) => {
                    let mut s = scene_t2.lock().unwrap();
                    s.circle_alpha = (t * 255.0) as u8;
                    s.circle_scale = t;
                    true
                }
                None => {
                    let mut s = scene_t2.lock().unwrap();
                    s.circle_alpha = 255;
                    s.circle_scale = 1.0;
                    false
                }
            }
        });
    }

    // — Ticker 3: progress bar (InOutQuad, 2.0 s, looping) -------------------
    {
        let scene_t3 = scene.clone();
        let mut anim = Animation::new(Duration::from_millis(2000))
            .ease(EaseMode::InOutQuad)
            .looping(true);

        canvas.animate(move |info| {
            if let Some(t) = anim.tick(info.delta) {
                scene_t3.lock().unwrap().bar_fill = t;
            }
            true // loops forever
        });
    }

    let mut layout = Box::new(BoxOrientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.run()?;

    Ok(())
}
