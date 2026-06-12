//! Fade-in animation example.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use aurea::elements::{Box, BoxOrientation};
use aurea::render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend};
use aurea::{AureaResult, Container, Window};
use aurea_animation::{Animation, EaseMode};

const W: u32 = 640;
const H: u32 = 480;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Animate — Fade In", W as i32, H as i32)?;

    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(30, 30, 30));

    let alpha: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.0));
    let alpha_draw = alpha.clone();
    let alpha_tick = alpha.clone();

    canvas.set_draw_callback(move |ctx| {
        let t = *alpha_draw.lock().unwrap();

        let label_paint = Paint::new()
            .color(Color::rgb(180, 180, 180))
            .style(PaintStyle::Fill);
        ctx.draw_text("OutCubic fade-in (1.5 s)", Point::new(20.0, 30.0), &label_paint)?;

        let a = (t * 255.0) as u8;
        let panel = Paint::new()
            .color(Color::rgba(80, 140, 220, a))
            .style(PaintStyle::Fill);
        ctx.draw_rect(Rect::new(100.0, 80.0, 440.0, 300.0), &panel)?;

        let border = Paint::new()
            .color(Color::rgba(255, 255, 255, 60))
            .style(PaintStyle::Stroke)
            .stroke_width(1.5);
        ctx.draw_rect(Rect::new(100.0, 80.0, 440.0, 300.0), &border)?;

        let pct = (t * 100.0) as u32;
        let pct_str = format!("{}%", pct);
        let pct_paint = Paint::new()
            .color(Color::rgb(220, 220, 220))
            .style(PaintStyle::Fill);
        ctx.draw_text(&pct_str, Point::new(20.0, H as f32 - 20.0), &pct_paint)?;

        Ok(())
    })?;

    let mut anim = Animation::new(Duration::from_millis(1500)).ease(EaseMode::OutCubic);

    canvas.animate(move |info| {
        match anim.tick(info.delta) {
            Some(t) => {
                *alpha_tick.lock().unwrap() = t;
                true
            }
            None => false,
        }
    });

    let mut layout = Box::new(BoxOrientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.run()?;

    Ok(())
}
