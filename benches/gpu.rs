//! Frame cost on the ZenGPU canvas, offscreen: zooming a tiled canvas under a label that changes,
//! with the slowest frame apart from the median because that is where a spike shows.

use std::sync::Arc;
use std::time::Instant;

use aurea::AureaResult;
use aurea::render::{
    Color, DrawingContext, Font, Image, Paint, Point, Rect, Renderer, Sampling, ZenGpuContext,
    ZenGpuRenderer,
};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1200;
const TILE: u32 = 256;

fn tile(seed: u32) -> Image {
    let data = (0..TILE * TILE)
        .flat_map(|i| {
            let shade = |by: u32| u8::try_from((i.wrapping_mul(by) ^ seed) % 251).unwrap_or(0);
            [shade(3), shade(5), shade(7), 255]
        })
        .collect();
    Image::new(TILE, TILE, data)
}

fn zoomed(
    ctx: &mut dyn DrawingContext,
    tiles: &[Image],
    zoom: f32,
    font: &Font,
) -> AureaResult<()> {
    ctx.clear(Color::rgb(20, 20, 24))?;
    ctx.set_sampling(Sampling::Smooth)?;
    let whole = Rect::new(0.0, 0.0, TILE as f32, TILE as f32);
    let side = TILE as f32 * zoom;
    for (i, image) in tiles.iter().enumerate() {
        let (row, col) = (i / 9, i % 9);
        let at = Rect::new(col as f32 * side, row as f32 * side, side, side);
        ctx.draw_image_region(image, whole, at)?;
    }
    let text = Paint::new().color(Color::rgb(230, 230, 230));
    let label = format!("{:.0}%", zoom * 100.0);
    ctx.draw_text_with_font(&label, Point::new(20.0, 30.0), font, &text)
}

fn report(name: &str, mut times: Vec<f64>) {
    let first = times[0];
    times.sort_by(f64::total_cmp);
    println!(
        "{name:<24} first {first:>8.2}  median {:>7.2}  slowest {:>8.2} ms",
        times[times.len() / 2],
        times[times.len() - 1]
    );
}

fn main() -> AureaResult<()> {
    let Ok(context) = ZenGpuContext::new() else {
        println!("no GPU here");
        return Ok(());
    };
    let mut gpu = ZenGpuRenderer::offscreen(Arc::new(context), WIDTH, HEIGHT, 1.0)?;
    let tiles: Vec<Image> = (0..72).map(tile).collect();
    let font = Font::new("", 14.0);

    let frame = |gpu: &mut ZenGpuRenderer, zoom: f32| -> AureaResult<(f64, f64)> {
        let start = Instant::now();
        let mut ctx = gpu.begin_frame()?;
        zoomed(&mut *ctx, &tiles, zoom, &font)?;
        drop(ctx);
        let recorded = start.elapsed().as_secs_f64() * 1e3;
        gpu.end_frame()?;
        Ok((recorded, start.elapsed().as_secs_f64() * 1e3))
    };

    let (_, upload) = frame(&mut gpu, 1.0)?;
    println!("{:<24} {upload:>8.2} ms", "first frame, all uploads");
    // A step per frame, each zoom seen before after the first pass.
    for pass in ["zoom, new labels", "zoom, seen labels"] {
        let mut recording = Vec::new();
        let mut totals = Vec::new();
        for step in 0..120_u16 {
            let zoom = 0.4 + f32::from(step % 60) * 0.02;
            let (recorded, total) = frame(&mut gpu, zoom)?;
            recording.push(recorded);
            totals.push(total);
        }
        report(pass, totals);
        report("  of it, recording", recording);
    }
    // What a zoom onto another pyramid level does: tiles the device has not seen.
    let (mut same_frame, mut settled) = (Vec::new(), Vec::new());
    for seed in 0..6_u32 {
        let fresh: Vec<Image> = (0..72).map(|i| tile(1000 * (seed + 1) + i)).collect();
        let start = Instant::now();
        let mut ctx = gpu.begin_frame()?;
        zoomed(&mut *ctx, &fresh, 1.0, &font)?;
        drop(ctx);
        gpu.end_frame()?;
        same_frame.push(start.elapsed().as_secs_f64() * 1e3);
        let start = Instant::now();
        let mut ctx = gpu.begin_frame()?;
        zoomed(&mut *ctx, &fresh, 1.01, &font)?;
        drop(ctx);
        gpu.end_frame()?;
        settled.push(start.elapsed().as_secs_f64() * 1e3);
    }
    report("72 new tiles", same_frame);
    report("  the frame after", settled);
    let mut pan = Vec::new();
    for _ in 0..120 {
        pan.push(frame(&mut gpu, 1.0)?.1);
    }
    report("unchanged", pan);
    Ok(())
}
