//! Fade-in animation example.

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

    let mut draw_canvas = canvas.clone();
    window.set_content(canvas)?;

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
            let label_paint = Paint::new()
                .color(Color::rgb(180, 180, 180))
                .style(PaintStyle::Fill);
            ctx.draw_text("OutCubic fade-in (1.5 s)", Point::new(20.0, 30.0), &label_paint)?;

            let a = (alpha * 255.0) as u8;
            let panel = Paint::new()
                .color(Color::rgba(80, 140, 220, a))
                .style(PaintStyle::Fill);
            ctx.draw_rect(Rect::new(100.0, 80.0, 440.0, 300.0), &panel)?;

            let border = Paint::new()
                .color(Color::rgba(255, 255, 255, 60))
                .style(PaintStyle::Stroke)
                .stroke_width(1.5);
            ctx.draw_rect(Rect::new(100.0, 80.0, 440.0, 300.0), &border)?;

            let pct = (alpha * 100.0) as u32;
            let pct_str = format!("{}%", pct);
            let pct_paint = Paint::new()
                .color(Color::rgb(220, 220, 220))
                .style(PaintStyle::Fill);
            ctx.draw_text(&pct_str, Point::new(20.0, H as f32 - 20.0), &pct_paint)?;

            Ok(())
        })?;

        window.process_frames()?;
        std::thread::sleep(Duration::from_millis(8));
    }

    Ok(())
}
