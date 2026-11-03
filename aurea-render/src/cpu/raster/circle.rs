//! Circles: analytic coverage, row by row.

use crate::cpu::blend::ConstSrc;
use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::cpu::raster::pixel;
use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped};
use crate::types::{BlendMode, Paint, PaintStyle, Point};

/// Padding, in pixels, on each outer-chord span edge so rounding never drops a pixel.
const STROKE_SPAN_PAD: i32 = 2;

/// Per-circle values shared by every row of a stroked circle.
struct CircleStrokeCtx<'a> {
    clip: ClipBox,
    center: Point,
    outer: f32,
    inner: f32,
    paint: &'a Paint,
    mode: BlendMode,
}

struct CircleFillCtx<'a> {
    clip: ClipBox,
    center: Point,
    radius: f32,
    paint: &'a Paint,
    mode: BlendMode,
    r_in: f32,
    r_out: f32,
    c_full: u32,
    opaque_fast: bool,
    const_src: Option<ConstSrc>,
}

pub(crate) fn draw(
    center: Point,
    radius: f32,
    paint: &Paint,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
) {
    let reach = match paint.style {
        PaintStyle::Fill => radius,
        PaintStyle::Stroke => radius + paint.stroke_width / 2.0,
    } + 1.0;
    let x0 = f32_to_u32_clamped((center.x - reach).floor().max(clip.left())).min(clip.x1);
    let y0 = f32_to_u32_clamped((center.y - reach).floor().max(clip.top())).min(clip.y1);
    let x1 = f32_to_u32_clamped((center.x + reach).ceil().max(clip.left())).min(clip.x1);
    let y1 = f32_to_u32_clamped((center.y + reach).ceil().max(clip.top())).min(clip.y1);

    match paint.style {
        PaintStyle::Fill => {
            fill_circle_region(center, radius, paint, mode, buf, bw, clip, x0, y0, x1, y1);
        }
        PaintStyle::Stroke => {
            stroke_circle_region(center, radius, paint, mode, buf, bw, clip, x0, y0, x1, y1);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_circle_region(
    center: Point,
    radius: f32,
    paint: &Paint,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
) {
    let ctx = CircleFillCtx {
        clip,
        center,
        radius,
        paint,
        mode,
        r_in: (radius - 0.5).max(0.0),
        r_out: radius + 0.5,
        c_full: pixel::color_to_u32(paint.color),
        opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
        const_src: (mode == BlendMode::Normal)
            .then(|| ConstSrc::new(pixel::color_to_u32(paint.color))),
    };

    for y in y0..y1 {
        fill_circle_row(buf, bw, &ctx, x0, x1, y);
    }
}

fn fill_circle_row(buf: &mut Band<'_>, bw: u32, ctx: &CircleFillCtx, x0: u32, x1: u32, y: u32) {
    let dy = y as f32 + 0.5 - ctx.center.y;
    if dy.abs() >= ctx.r_out {
        return;
    }
    let half_out = (ctx.r_out * ctx.r_out - dy * dy).max(0.0).sqrt();
    let xo0 = f32_to_i32_clamped((ctx.center.x - half_out).floor().max(x0 as f32)).min(x1 as i32);
    let xo1 = f32_to_i32_clamped((ctx.center.x + half_out).ceil().min(x1 as f32)).max(x0 as i32);

    let (xi0, xi1) = if dy.abs() < ctx.r_in {
        let half_in = (ctx.r_in * ctx.r_in - dy * dy).max(0.0).sqrt();
        let a = f32_to_i32_clamped((ctx.center.x - half_in).ceil());
        let b = f32_to_i32_clamped((ctx.center.x + half_in).floor());
        if a < b {
            (a.clamp(xo0, xo1), b.clamp(xo0, xo1))
        } else {
            (xo0, xo0)
        }
    } else {
        (xo0, xo0)
    };

    fill_circle_edge_span(buf, bw, ctx, xo0, xi0, y);

    if xi0 < xi1 {
        let row_start = (y * bw) as usize;
        let span = row_start + xi0.unsigned_abs() as usize..row_start + xi1.unsigned_abs() as usize;
        if ctx.opaque_fast {
            buf[span].fill(ctx.c_full);
        } else if let (Some(src), Some(pixels)) = (ctx.const_src, buf.get_mut(span)) {
            src.over_span(pixels);
        } else {
            for x in xi0..xi1 {
                pixel::put(buf, bw, ctx.clip, x, y as i32, ctx.c_full, ctx.mode);
            }
        }
    }

    fill_circle_edge_span(buf, bw, ctx, xi1, xo1, y);
}

fn fill_circle_edge_span(
    buf: &mut Band<'_>,
    bw: u32,
    ctx: &CircleFillCtx,
    xa: i32,
    xb: i32,
    y: u32,
) {
    for x in xa..xb {
        let cov = circle_coverage(ctx.center, ctx.radius, x as f32, y as f32);
        if cov <= 0.0 {
            continue;
        }
        match ctx.const_src {
            Some(src) if ctx.clip.contains(x, y as i32) => {
                let alpha = f32_to_u32_clamped((f32::from(ctx.paint.color.a) * cov).round());
                if let Some(p) = buf.get_mut((y * bw) as usize + x.unsigned_abs() as usize) {
                    *p = src.over_at_alpha(*p, alpha);
                }
            }
            Some(_) => {}
            None => {
                let c = pixel::color_to_u32_with_coverage(ctx.paint.color, cov);
                pixel::put(buf, bw, ctx.clip, x, y as i32, c, ctx.mode);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn stroke_circle_region(
    center: Point,
    radius: f32,
    paint: &Paint,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
) {
    let half = paint.stroke_width / 2.0;
    if half <= 0.0 {
        return;
    }
    let ctx = CircleStrokeCtx {
        clip,
        center,
        outer: radius + half,
        inner: radius - half,
        paint,
        mode,
    };
    for y in y0..y1 {
        stroke_circle_row(buf, bw, &ctx, x0, x1, y);
    }
}

/// Splits a row into the spans that can hold stroke pixels: the outer chord, minus the
/// inner chord when it is wide enough to skip. Pixels outside those spans have no coverage,
/// so they are never tested.
fn stroke_circle_row(
    buf: &mut Band<'_>,
    bw: u32,
    ctx: &CircleStrokeCtx<'_>,
    x0: u32,
    x1: u32,
    y: u32,
) {
    let dy = y as f32 + 0.5 - ctx.center.y;
    let chord = |r: f32| (r * r - dy * dy).max(0.0).sqrt();
    let half_out = chord(ctx.outer + 0.5);
    let pad = STROKE_SPAN_PAD;
    let lo = (f32_to_i32_clamped((ctx.center.x - half_out).floor()) - pad).max(x0 as i32);
    let hi = (f32_to_i32_clamped((ctx.center.x + half_out).ceil()) + pad).min(x1 as i32);
    if lo >= hi {
        return;
    }
    let half_in = chord(ctx.inner - 0.5);
    if half_in < 1.0 {
        stroke_circle_span(buf, bw, ctx, y, lo, hi);
        return;
    }
    // Centres in [skip_lo, skip_hi) lie at least a pixel inside the hole.
    let skip_lo = f32_to_i32_clamped((ctx.center.x - half_in + 0.5).ceil());
    let skip_hi = f32_to_i32_clamped((ctx.center.x + half_in - 1.5).floor()) + 1;
    stroke_circle_span(buf, bw, ctx, y, lo, skip_lo.min(hi));
    stroke_circle_span(buf, bw, ctx, y, skip_hi.max(lo), hi);
}

fn stroke_circle_span(
    buf: &mut Band<'_>,
    bw: u32,
    ctx: &CircleStrokeCtx<'_>,
    y: u32,
    xa: i32,
    xb: i32,
) {
    let dy = y as f32 + 0.5 - ctx.center.y;
    for x in xa..xb {
        let dx = x as f32 + 0.5 - ctx.center.x;
        let d = (dx * dx + dy * dy).sqrt();
        let cov = ((ctx.outer + 0.5 - d).clamp(0.0, 1.0) - (ctx.inner + 0.5 - d).clamp(0.0, 1.0))
            .max(0.0);
        if cov > 0.0 {
            let c = pixel::color_to_u32_with_coverage(ctx.paint.color, cov);
            pixel::put(buf, bw, ctx.clip, x, y as i32, c, ctx.mode);
        }
    }
}

fn circle_coverage(center: Point, radius: f32, px: f32, py: f32) -> f32 {
    let cx = px + 0.5;
    let cy = py + 0.5;
    let d = ((cx - center.x).powi(2) + (cy - center.y).powi(2)).sqrt();
    if d >= radius + 0.5 {
        return 0.0;
    }
    if d <= radius - 0.5 {
        return 1.0;
    }
    (radius + 0.5 - d).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;
    use crate::cpu::raster::fill::PathScratch;
    use crate::cpu::raster::item;
    use crate::types::{BlendMode, Color, Paint, PaintStyle, Path, PathCommand, Point};
    use std::f32::consts::{PI, TAU};

    const SIDE: u32 = 128;

    fn covered(radius: f32, paint: &Paint) -> f32 {
        let mut px = vec![0_u32; (SIDE * SIDE) as usize];
        super::draw(
            Point::new(64.25, 63.5),
            radius,
            paint,
            BlendMode::Normal,
            &mut Band::whole(&mut px),
            SIDE,
            ClipBox::surface(SIDE, SIDE),
        );
        px.iter().map(|p| (p >> 24) as f32 / 255.0).sum()
    }

    #[test]
    fn a_filled_circle_covers_its_area() {
        let white = Paint::new().color(Color::rgb(255, 255, 255));
        let (area, want) = (covered(40.0, &white), PI * 1600.0);
        assert!((area - want).abs() < want * 0.01, "{area} and {want}");
    }

    #[test]
    fn a_stroked_circle_matches_its_path() {
        let line = Paint::new()
            .color(Color::rgb(255, 255, 255))
            .style(PaintStyle::Stroke)
            .stroke_width(3.0);
        let center = Point::new(64.25, 63.5);
        let surface = ClipBox::surface(SIDE, SIDE);
        let mut direct = vec![0_u32; (SIDE * SIDE) as usize];
        super::draw(
            center,
            40.0,
            &line,
            BlendMode::Normal,
            &mut Band::whole(&mut direct),
            SIDE,
            surface,
        );
        let mut path = Path::new();
        path.arc(center, 40.0, (0.0, TAU), false);
        path.commands.push(PathCommand::Close);
        let mut traced = vec![0_u32; (SIDE * SIDE) as usize];
        item::draw_path(
            &path,
            &line,
            BlendMode::Normal,
            &mut Band::whole(&mut traced),
            &mut PathScratch::default(),
            SIDE,
            surface,
        )
        .expect("path");
        let far = direct
            .iter()
            .zip(&traced)
            .filter(|(a, b)| (*a >> 24).abs_diff(*b >> 24) > 64)
            .count();
        assert_eq!(far, 0, "pixels far from the path's");
    }

    #[test]
    fn a_stroked_circle_covers_its_ring() {
        let line = Paint::new()
            .color(Color::rgb(255, 255, 255))
            .style(PaintStyle::Stroke)
            .stroke_width(3.0);
        let (area, want) = (covered(40.0, &line), PI * (41.5 * 41.5 - 38.5 * 38.5));
        assert!((area - want).abs() < want * 0.02, "{area} and {want}");
    }
}
