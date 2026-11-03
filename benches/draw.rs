//! Cost of one full repaint on the CPU rasterizer, per kind of draw, with no window. Each frame
//! changes the clear colour, so the damage tracker cannot skip anything.

use std::collections::hash_map::DefaultHasher;
use std::f32::consts::FRAC_PI_8;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use aurea::AureaResult;
use aurea::render::{
    Color, CpuRasterizer, DrawingContext, Font, GradientStop, Image, LinearGradient, Paint,
    PaintStyle, Path, PathCommand, Point, Rect, Renderer, Sampling,
};

mod support;

#[global_allocator]
static GLOBAL: support::Counting = support::Counting;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1200;
const FRAMES: u32 = 60;

type Draw<'a> = &'a dyn Fn(&mut dyn DrawingContext) -> AureaResult<()>;

struct Repaint {
    median: f64,
    hash: u64,
    /// Allocations a frame once everything has warmed up.
    allocations: f64,
}

fn repaint(threads: Option<usize>, draw: Draw<'_>) -> AureaResult<Repaint> {
    let mut raster = CpuRasterizer::new(WIDTH, HEIGHT);
    raster.set_threads(threads);
    let mut times = Vec::with_capacity(FRAMES as usize);
    let mut before = 0;
    for frame in 0..FRAMES {
        if frame == FRAMES / 2 {
            before = support::allocations();
        }
        let shade = u8::try_from(frame % 2).unwrap_or(0);
        let start = Instant::now();
        let mut ctx = raster.begin_frame()?;
        ctx.clear(Color::rgb(20 + shade, 20, 24))?;
        draw(&mut *ctx)?;
        drop(ctx);
        raster.end_frame()?;
        times.push(start.elapsed().as_secs_f64() * 1e3);
    }
    let allocations = (support::allocations() - before) as f64 / f64::from(FRAMES - FRAMES / 2);
    times.sort_by(f64::total_cmp);
    let mut hash = DefaultHasher::new();
    raster.pixels().hash(&mut hash);
    Ok(Repaint {
        median: times[times.len() / 2],
        hash: hash.finish(),
        allocations,
    })
}

fn run(name: &str, draw: Draw<'_>) -> AureaResult<()> {
    let one = repaint(Some(1), draw)?;
    let four = repaint(Some(4), draw)?;
    let all = repaint(None, draw)?;
    println!(
        "{name:<20} {:>7.2} {:>7.2} {:>7.2} ms {:>7.1} alloc  {:016x}",
        one.median, four.median, all.median, one.allocations, one.hash
    );
    Ok(())
}

fn grid(step: f32) -> impl Iterator<Item = (f32, f32)> {
    let (w, h) = (WIDTH as f32, HEIGHT as f32);
    (0..)
        .map(move |row| row as f32 * step)
        .take_while(move |y| *y < h)
        .flat_map(move |y| {
            (0..)
                .map(move |col| col as f32 * step)
                .take_while(move |x| *x < w)
                .map(move |x| (x, y))
        })
}

fn round_rect_path(r: Rect, radius: f32) -> Path {
    let k = radius * 0.447_715;
    let (l, t, rr, b) = (r.x, r.y, r.x + r.width, r.y + r.height);
    let p = Point::new;
    Path {
        commands: vec![
            PathCommand::MoveTo(p(l + radius, t)),
            PathCommand::LineTo(p(rr - radius, t)),
            PathCommand::CubicTo(p(rr - k, t), p(rr, t + k), p(rr, t + radius)),
            PathCommand::LineTo(p(rr, b - radius)),
            PathCommand::CubicTo(p(rr, b - k), p(rr - k, b), p(rr - radius, b)),
            PathCommand::LineTo(p(l + radius, b)),
            PathCommand::CubicTo(p(l + k, b), p(l, b - k), p(l, b - radius)),
            PathCommand::LineTo(p(l, t + radius)),
            PathCommand::CubicTo(p(l, t + k), p(l + k, t), p(l + radius, t)),
            PathCommand::Close,
        ],
        fill_rule: Default::default(),
    }
}

fn main() -> AureaResult<()> {
    let solid = Paint::new().color(Color::rgb(90, 140, 200));
    let glass = Paint::new().color(Color::rgba(90, 140, 200, 128));
    let line = Paint::new()
        .color(Color::rgb(200, 200, 210))
        .style(PaintStyle::Stroke)
        .stroke_width(1.0);
    let glass_line = Paint::new()
        .color(Color::rgba(200, 200, 210, 128))
        .style(PaintStyle::Stroke)
        .stroke_width(1.0);
    let font = Font::new("", 14.0);
    let text = Paint::new().color(Color::rgb(230, 230, 230));
    let glass_text = Paint::new().color(Color::rgba(230, 230, 230, 180));

    let side = 512_u32;
    let mut pixels = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let half = |v: u32| u8::try_from(v / 2).unwrap_or(u8::MAX);
            pixels.extend_from_slice(&[half(x), half(y), half(x ^ y), 255]);
        }
    }
    let image = Image::new(side, side, pixels);
    let whole = Rect::new(0.0, 0.0, side as f32, side as f32);
    let gradient = LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(800.0, 500.0),
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgba(255, 0, 0, 200),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgba(0, 0, 255, 200),
            },
        ],
    };

    println!("{:<20} {:>7} {:>7} {:>7}", "", "1", "4", "auto");
    run("clear", &|_| Ok(()))?;
    run("rects", &|c| {
        grid(24.0).try_for_each(|(x, y)| c.draw_rect(Rect::new(x, y, 20.0, 20.0), &solid))
    })?;
    run("stroke rects", &|c| {
        grid(40.0).try_for_each(|(x, y)| c.draw_rect(Rect::new(x, y, 36.0, 36.0), &line))
    })?;
    run("glass stroke rects", &|c| {
        grid(40.0).try_for_each(|(x, y)| c.draw_rect(Rect::new(x, y, 36.0, 36.0), &glass_line))
    })?;
    run("round rects", &|c| {
        grid(40.0)
            .try_for_each(|(x, y)| c.draw_round_rect(Rect::new(x, y, 36.0, 36.0), 6.0, &glass))
    })?;
    run("circles", &|c| {
        grid(40.0)
            .try_for_each(|(x, y)| c.draw_circle(Point::new(x + 18.0, y + 18.0), 16.0, &glass))
    })?;
    run("path fills", &|c| {
        grid(60.0).try_for_each(|(x, y)| {
            c.draw_path(&round_rect_path(Rect::new(x, y, 56.0, 56.0), 12.0), &glass)
        })
    })?;
    run("path strokes", &|c| {
        grid(60.0).try_for_each(|(x, y)| {
            c.draw_path(&round_rect_path(Rect::new(x, y, 56.0, 56.0), 12.0), &line)
        })
    })?;
    run("text", &|c| {
        (0..55).try_for_each(|row| {
            let at = Point::new(10.0, 20.0 + row as f32 * 21.0);
            c.draw_text_with_font(LINE, at, &font, &text)
        })
    })?;
    run("glass text", &|c| {
        (0..55).try_for_each(|row| {
            let at = Point::new(10.0, 20.0 + row as f32 * 21.0);
            c.draw_text_with_font(LINE, at, &font, &glass_text)
        })
    })?;
    run("gradient", &|c| {
        c.fill_linear_gradient(&gradient, Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32))
    })?;
    run("image tiles 1:1", &|c| {
        grid(512.0).try_for_each(|(x, y)| {
            c.draw_image_region(&image, whole, Rect::new(x, y, 512.0, 512.0))
        })
    })?;
    run("image tiles 150%", &|c| {
        c.set_sampling(Sampling::Smooth)?;
        grid(768.0).try_for_each(|(x, y)| {
            c.draw_image_region(&image, whole, Rect::new(x, y, 768.0, 768.0))
        })
    })?;
    run("image tiles 67%", &|c| {
        c.set_sampling(Sampling::Smooth)?;
        grid(341.0).try_for_each(|(x, y)| {
            c.draw_image_region(&image, whole, Rect::new(x, y, 341.0, 341.0))
        })
    })?;
    run("image turned", &|c| {
        c.set_sampling(Sampling::Smooth)?;
        c.translate(960.0, 600.0)?;
        c.rotate(FRAC_PI_8)?;
        c.scale(2.5, 2.5)?;
        c.draw_image_region(&image, whole, Rect::new(-256.0, -256.0, 512.0, 512.0))
    })?;
    run("image turned small", &|c| {
        c.set_sampling(Sampling::Smooth)?;
        c.translate(960.0, 600.0)?;
        c.rotate(FRAC_PI_8)?;
        c.scale(0.6, 0.6)?;
        c.draw_image_region(&image, whole, Rect::new(-256.0, -256.0, 512.0, 512.0))
    })?;
    run("round clip rects", &|c| {
        c.clip_path(&round_rect_path(
            Rect::new(40.0, 40.0, 1840.0, 1120.0),
            16.0,
        ))?;
        grid(24.0).try_for_each(|(x, y)| c.draw_rect(Rect::new(x, y, 20.0, 20.0), &solid))
    })?;
    run("clips taking turns", &|c| {
        let left = round_rect_path(Rect::new(10.0, 10.0, 940.0, 1180.0), 16.0);
        let right = round_rect_path(Rect::new(970.0, 10.0, 940.0, 1180.0), 16.0);
        (0..200).try_for_each(|i| {
            let y = (i % 20) as f32 * 55.0 + 20.0;
            let x = (i / 20) as f32 * 90.0 + 20.0;
            c.save()?;
            c.clip_path(&left)?;
            c.draw_rect(Rect::new(x, y, 80.0, 40.0), &solid)?;
            c.restore()?;
            c.save()?;
            c.clip_path(&right)?;
            c.draw_rect(Rect::new(x + 960.0, y, 80.0, 40.0), &solid)?;
            c.restore()
        })
    })?;
    run("round clip image", &|c| {
        c.clip_path(&round_rect_path(
            Rect::new(40.0, 40.0, 1840.0, 1120.0),
            16.0,
        ))?;
        c.draw_image_region(
            &image,
            whole,
            Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32),
        )
    })?;
    Ok(())
}

const LINE: &str = "The quick brown fox jumps over the lazy dog and keeps going past the end of a long line 0123456789";
