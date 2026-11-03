//! Frame-timing benchmark for the P6-A damage/tile-cache pipeline.

use std::time::Instant;

use aurea::AureaResult;
use aurea::render::{
    Color, CpuRasterizer, DrawingContext, Font, Image, Paint, Point, Rect, Renderer,
};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
const FRAMES: u32 = 1000;
const GRID: u32 = 20;

fn draw_background(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
    ctx.clear(Color::rgb(20, 20, 24))?;
    let cell_w = WIDTH as f32 / GRID as f32;
    let cell_h = HEIGHT as f32 / GRID as f32;
    for row in 0..GRID {
        for col in 0..GRID {
            let paint = Paint::new().color(Color::rgb(
                u8::try_from((row * 13) % 256).expect("mod 256 fits in u8"),
                u8::try_from((col * 7) % 256).expect("mod 256 fits in u8"),
                u8::try_from(((row + col) * 5) % 256).expect("mod 256 fits in u8"),
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
    Ok(start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames))
}

/// A canvas of tiles under a panel where only one label's text changes: the cost should follow
/// the label, not the canvas.
fn label_on_a_canvas() -> AureaResult<()> {
    const SIDE: u32 = 256;
    let (width, height) = (2732, 2048);
    let pixels: Vec<u8> = (0..SIDE * SIDE)
        .flat_map(|i| [u8::try_from(i % 251).unwrap_or(0), 90, 140, 255])
        .collect();
    let image = Image::new(SIDE, SIDE, pixels);
    let whole = Rect::new(0.0, 0.0, SIDE as f32, SIDE as f32);
    let font = Font::new("", 14.0);
    let panel = Paint::new().color(Color::rgb(36, 38, 44));
    let text = Paint::new().color(Color::rgb(230, 230, 230));
    let mut r = CpuRasterizer::new(width, height);
    r.set_counting(true);
    let (mut slowest, mut total) = (0.0_f64, 0.0_f64);
    let (mut dirty, mut drawn) = (0, 0);
    let frames = 120_u32;
    for frame in 0..frames {
        let start = Instant::now();
        let mut ctx = r.begin_frame()?;
        ctx.clear(Color::rgb(20, 20, 24))?;
        for row in 0..8 {
            for col in 0..11 {
                let at = Rect::new(col as f32 * 256.0, row as f32 * 256.0, 256.0, 256.0);
                ctx.draw_image_region(&image, whole, at)?;
            }
        }
        ctx.draw_rect(Rect::new(2332.0, 0.0, 400.0, 2048.0), &panel)?;
        let label = format!("Exposure {:.2}", f64::from(frame) / 100.0);
        ctx.draw_text_with_font(&label, Point::new(2350.0, 60.0), &font, &text)?;
        drop(ctx);
        r.end_frame()?;
        if frame > 0 {
            let took = start.elapsed().as_secs_f64() * 1000.0;
            total += took;
            slowest = slowest.max(took);
            if let Some(counters) = r.frame_counters() {
                dirty = dirty.max(counters.dirty_tiles);
                drawn = drawn.max(counters.drawn);
            }
        }
    }
    println!(
        "label on a canvas:  {:.4} ms/frame, slowest {slowest:.2}, at most {dirty} dirty tiles, {drawn} items drawn",
        total / f64::from(frames - 1)
    );
    Ok(())
}

fn main() -> AureaResult<()> {
    let mut r = CpuRasterizer::new(WIDTH, HEIGHT);
    let ms_static = run_frames(&mut r, FRAMES, |ctx, _| draw_background(ctx))?;
    println!("unchanged scene:    {ms_static:.4} ms/frame ({FRAMES} frames, {GRID}x{GRID} grid)");

    let mut r = CpuRasterizer::new(WIDTH, HEIGHT);
    let cursor = Paint::new().color(Color::rgb(255, 64, 64));
    let ms_moving = run_frames(&mut r, FRAMES, |ctx, frame| {
        draw_background(ctx)?;
        let x = (frame % WIDTH) as f32;
        ctx.draw_rect(Rect::new(x, HEIGHT as f32 / 2.0, 16.0, 16.0), &cursor)
    })?;
    println!("moving-rect scene:  {ms_moving:.4} ms/frame ({FRAMES} frames)");

    label_on_a_canvas()?;

    Ok(())
}
