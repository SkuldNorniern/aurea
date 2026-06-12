//! Multiple simultaneous animations example.
//!
//! Three independent animations run at the same time on the same canvas:
//! - A square slides in from the left  (OutQuint, 1.2 s)
//! - A circle fades + scales in        (OutCubic, 0.8 s, delayed 0.4 s)
//! - A progress bar fills up           (InOutQuad, 2.0 s, looping)
//!
//! Demonstrates:
//! - Multiple concurrent `Animation`s ticked from one application loop
//! - Manual animation delay by accumulating time before the first tick
//! - The poll-loop render pattern: `poll_events` → `draw` → `process_frames`

use std::time::{Duration, Instant};

use aurea::render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend};
use aurea::{AureaResult, Window, WindowEvent};
use aurea_animation::{Animation, EaseMode};

const W: u32 = 700;
const H: u32 = 500;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Animate — Multiple Simultaneous", W as i32, H as i32)?;

    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(245, 245, 248));

    let mut draw_canvas = canvas.clone();
    window.set_content(canvas)?;

    // — Animation 1: sliding square (OutQuint, 1.2 s) -------------------------
    let slide_start = -(200.0_f32);
    let slide_end = 40.0_f32;
    let mut slide_anim = Animation::new(Duration::from_millis(1200)).ease(EaseMode::OutQuint);
    let mut slide_x = slide_start;

    // — Animation 2: fading circle (OutCubic, 0.8 s, with 0.4 s delay) --------
    let circle_delay = Duration::from_millis(400);
    let mut circle_waited = Duration::ZERO;
    let mut circle_anim = Animation::new(Duration::from_millis(800)).ease(EaseMode::OutCubic);
    let mut circle_alpha: u8 = 0;
    let mut circle_scale: f32 = 0.0;

    // — Animation 3: progress bar (InOutQuad, 2.0 s, looping) -----------------
    let mut bar_anim = Animation::new(Duration::from_millis(2000))
        .ease(EaseMode::InOutQuad)
        .looping(true);
    let mut bar_fill: f32 = 0.0;

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

        if let Some(t) = slide_anim.tick(delta) {
            slide_x = slide_start + (slide_end - slide_start) * t;
        }

        // Delay phase: accumulate time before the circle starts animating.
        if circle_waited < circle_delay {
            circle_waited += delta;
        } else if let Some(t) = circle_anim.tick(delta) {
            circle_alpha = (t * 255.0) as u8;
            circle_scale = t;
        }

        if let Some(t) = bar_anim.tick(delta) {
            bar_fill = t;
        }

        draw_canvas.draw(|ctx| {
            // ── Sliding square ────────────────────────────────────────────────
            let sq = Paint::new()
                .color(Color::rgb(80, 120, 200))
                .style(PaintStyle::Fill);
            ctx.draw_rect(Rect::new(slide_x, 60.0, 200.0, 200.0), &sq)?;
            let sq_label = Paint::new()
                .color(Color::rgb(255, 255, 255))
                .style(PaintStyle::Fill);
            ctx.draw_text("Slide (OutQuint)", Point::new(slide_x + 10.0, 165.0), &sq_label)?;

            // ── Fading + scaling circle ───────────────────────────────────────
            let cx = W as f32 * 0.65;
            let cy = 165.0_f32;
            let r = 80.0 * circle_scale;
            if r > 0.5 {
                let circle = Paint::new()
                    .color(Color::rgba(200, 80, 80, circle_alpha))
                    .style(PaintStyle::Fill);
                ctx.draw_circle(Point::new(cx, cy), r, &circle)?;
                let ci_label = Paint::new()
                    .color(Color::rgba(255, 255, 255, circle_alpha))
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

            let bar_paint = Paint::new()
                .color(Color::rgb(60, 180, 110))
                .style(PaintStyle::Fill);
            ctx.draw_rect(Rect::new(bar_x, bar_y, bar_w * bar_fill, bar_h), &bar_paint)?;

            let border = Paint::new()
                .color(Color::rgb(160, 160, 165))
                .style(PaintStyle::Stroke)
                .stroke_width(1.0);
            ctx.draw_rect(Rect::new(bar_x, bar_y, bar_w, bar_h), &border)?;

            let pct = format!("{}% (InOutQuad, looping)", (bar_fill * 100.0) as u32);
            let bar_label = Paint::new()
                .color(Color::rgb(60, 60, 60))
                .style(PaintStyle::Fill);
            ctx.draw_text(&pct, Point::new(bar_x, bar_y - 18.0), &bar_label)?;

            Ok(())
        })?;

        window.process_frames()?;
        std::thread::sleep(Duration::from_millis(8));
    }

    Ok(())
}
