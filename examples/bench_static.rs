//! Frame-timing benchmark for the P6-A damage/tile-cache pipeline.
//!
//! Drives `CpuRasterizer` directly (no window/event loop) through 1000
//! frames of an unchanged complex scene, then 1000 frames of the same
//! scene with one small rect sweeping across the canvas. Run with:
//!
//! ```text
//! cargo run --release --example bench_static
//! ```

use std::time::Instant;

use aurea::AureaResult;
use aurea::render::{Color, CpuRasterizer, DrawingContext, Paint, Rect, Renderer};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
const FRAMES: u32 = 1000;
const GRID: u32 = 20;

/// A "complex" static scene: a background clear plus a `GRID x GRID` grid
/// of distinctly-colored rects.
fn draw_background(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
    ctx.clear(Color::rgb(20, 20, 24))?;
    let cell_w = WIDTH as f32 / GRID as f32;
    let cell_h = HEIGHT as f32 / GRID as f32;
    for row in 0..GRID {
        for col in 0..GRID {
            let paint = Paint::new().color(Color::rgb(
                ((row * 13) % 256) as u8,
                ((col * 7) % 256) as u8,
                (((row + col) * 5) % 256) as u8,
            ));
            ctx.draw_rect(
                Rect::new(
                    col as f32 * cell_w + 2.0,
                    row as f32 * cell_h + 2.0,
                    cell_w - 4.0,
                    cell_h - 4.0,
                ),
                &paint,
            )?;
        }
    }
    Ok(())
}

fn run_frames(
    r: &mut CpuRasterizer,
    frames: u32,
    mut draw: impl FnMut(&mut dyn DrawingContext, u32) -> AureaResult<()>,
) -> AureaResult<f64> {
    let start = Instant::now();
    for frame in 0..frames {
        let mut ctx = r.begin_frame()?;
        draw(ctx.as_mut(), frame)?;
        drop(ctx);
        r.end_frame()?;
    }
    Ok(start.elapsed().as_secs_f64() * 1000.0 / frames as f64)
}

fn main() -> AureaResult<()> {
    // Identical display list every frame: after frame 1, stage-1's diff
    // sees `Unchanged` and `end_frame` early-returns with zero pixels
    // touched, regardless of how many items the scene contains.
    let mut r = CpuRasterizer::new(WIDTH, HEIGHT);
    let ms_static = run_frames(&mut r, FRAMES, |ctx, _| draw_background(ctx))?;
    println!("unchanged scene:    {ms_static:.4} ms/frame ({FRAMES} frames, {GRID}x{GRID} grid)");

    // Same static background plus a small rect that sweeps one pixel per
    // frame. Only the tile(s) the rect's old/new bounds overlap should be
    // marked dirty and redrawn.
    let mut r = CpuRasterizer::new(WIDTH, HEIGHT);
    let cursor = Paint::new().color(Color::rgb(255, 64, 64));
    let ms_moving = run_frames(&mut r, FRAMES, |ctx, frame| {
        draw_background(ctx)?;
        let x = (frame % WIDTH) as f32;
        ctx.draw_rect(Rect::new(x, HEIGHT as f32 / 2.0, 16.0, 16.0), &cursor)
    })?;
    println!("moving-rect scene:  {ms_moving:.4} ms/frame ({FRAMES} frames)");

    Ok(())
}
