//! Rects: analytic coverage at the edges, spans between them.

use crate::cpu::blend::ConstSrc;
use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::cpu::raster::pixel;
use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped};
use crate::types::{BlendMode, Paint, PaintStyle, Rect};

struct RectFillCtx<'a> {
    clip: ClipBox,
    paint: &'a Paint,
    mode: BlendMode,
    xl: f32,
    xr: f32,
    c_full: u32,
    opaque_fast: bool,
    const_src: Option<ConstSrc>,
}

pub(crate) fn clear(rect: &Rect, color: u32, buf: &mut Band<'_>, bw: u32, clip: ClipBox) {
    if clip.is_empty() {
        return;
    }
    let x0 = f32_to_u32_clamped(rect.x.floor().clamp(clip.left(), clip.right()));
    let y0 = f32_to_u32_clamped(rect.y.floor().clamp(clip.top(), clip.bottom()));
    let x1 = f32_to_u32_clamped(
        (rect.x + rect.width)
            .ceil()
            .clamp(clip.left(), clip.right()),
    );
    let y1 = f32_to_u32_clamped(
        (rect.y + rect.height)
            .ceil()
            .clamp(clip.top(), clip.bottom()),
    );

    for y in y0..y1 {
        let start = (y * bw + x0) as usize;
        let end = (y * bw + x1) as usize;
        buf[start..end].fill(color);
    }
}

pub(crate) fn draw(
    rect: &Rect,
    paint: &Paint,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
) {
    let reach = match paint.style {
        PaintStyle::Fill => 0.0,
        PaintStyle::Stroke => paint.stroke_width / 2.0,
    };
    let x0 = f32_to_u32_clamped((rect.x - reach).floor().max(clip.left())).min(clip.x1);
    let y0 = f32_to_u32_clamped((rect.y - reach).floor().max(clip.top())).min(clip.y1);
    let x1 = f32_to_u32_clamped((rect.x + rect.width + reach).ceil().max(clip.left())).min(clip.x1);
    let y1 = f32_to_u32_clamped((rect.y + rect.height + reach).ceil().max(clip.top())).min(clip.y1);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    match paint.style {
        PaintStyle::Fill => fill_rect_region(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1),
        PaintStyle::Stroke => stroke_rect_region(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1),
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rect_region(
    rect: &Rect,
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
    if paint.color.a == 255 && mode == BlendMode::Normal && on_pixel_grid(rect) {
        let c = pixel::color_to_u32(paint.color);
        for y in y0..y1 {
            let start = (y * bw + x0) as usize;
            let end = (y * bw + x1) as usize;
            if end <= buf.len() {
                buf[start..end].fill(c);
            }
        }
    } else {
        fill_rect_region_translucent(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1);
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rect_region_translucent(
    rect: &Rect,
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
    let xl = rect.x;
    let xr = rect.x + rect.width;
    let yl = rect.y;
    let yr = rect.y + rect.height;

    let xi0 = f32_to_i32_clamped(xl.ceil())
        .clamp(x0 as i32, x1 as i32)
        .unsigned_abs();
    let xi1 = f32_to_i32_clamped(xr.floor())
        .clamp(x0 as i32, x1 as i32)
        .unsigned_abs();
    let has_full_x = xi0 < xi1;
    let yi0 = f32_to_i32_clamped(yl.ceil())
        .clamp(y0 as i32, y1 as i32)
        .unsigned_abs();
    let yi1 = f32_to_i32_clamped(yr.floor())
        .clamp(y0 as i32, y1 as i32)
        .unsigned_abs();

    let const_src =
        (mode == BlendMode::Normal).then(|| ConstSrc::new(pixel::color_to_u32(paint.color)));
    let ctx = RectFillCtx {
        clip,
        paint,
        mode,
        xl,
        xr,
        c_full: pixel::color_to_u32(paint.color),
        opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
        const_src,
    };

    for y in y0..y1 {
        let row_cov = if y >= yi0 && y < yi1 {
            1.0
        } else {
            rect_cov_y(y, yl, yr)
        };
        if row_cov <= 0.0 {
            continue;
        }
        fill_rect_row_translucent(buf, bw, &ctx, x0, x1, xi0, xi1, has_full_x, y, row_cov);
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rect_row_translucent(
    buf: &mut Band<'_>,
    bw: u32,
    ctx: &RectFillCtx,
    x0: u32,
    x1: u32,
    xi0: u32,
    xi1: u32,
    has_full_x: bool,
    y: u32,
    row_cov: f32,
) {
    if !has_full_x {
        fill_rect_edge_span(buf, bw, ctx, x0, x1, y, row_cov);
        return;
    }

    fill_rect_edge_span(buf, bw, ctx, x0, xi0, y, row_cov);

    if row_cov >= 1.0 {
        if ctx.opaque_fast {
            let row_start = (y * bw) as usize;
            buf[row_start + xi0 as usize..row_start + xi1 as usize].fill(ctx.c_full);
        } else if let Some(src) = ctx.const_src {
            let row_start = (y * bw) as usize;
            let span = row_start + xi0 as usize..row_start + xi1 as usize;
            if let Some(pixels) = buf.get_mut(span) {
                src.over_span(pixels);
            }
        } else {
            for x in xi0..xi1 {
                pixel::put(buf, bw, ctx.clip, x as i32, y as i32, ctx.c_full, ctx.mode);
            }
        }
    } else {
        blend_span_at(buf, bw, ctx, xi0, xi1, y, row_cov);
    }

    fill_rect_edge_span(buf, bw, ctx, xi1, x1, y, row_cov);
}

/// [`blend_at`] over `xa..xb` on row `y`, where every pixel has coverage `cov`.
fn blend_span_at(
    buf: &mut Band<'_>,
    bw: u32,
    ctx: &RectFillCtx,
    xa: u32,
    xb: u32,
    y: u32,
    cov: f32,
) {
    if let Some(src) = ctx.const_src {
        // Only the part of the span inside the clip is drawn.
        let clip = ctx.clip;
        let (sa, sb) = (xa.max(clip.x0), xb.min(clip.x1));
        if y < clip.y0 || y >= clip.y1 || sa >= sb {
            return;
        }
        let alpha = f32_to_u32_clamped((f32::from(ctx.paint.color.a) * cov).round());
        let row_start = (y * bw) as usize;
        if let Some(pixels) = buf.get_mut(row_start + sa as usize..row_start + sb as usize) {
            src.over_span_at_alpha(pixels, alpha);
            return;
        }
    }
    for x in xa..xb {
        blend_at(buf, bw, ctx, x, y, cov);
    }
}

fn blend_at(buf: &mut Band<'_>, bw: u32, ctx: &RectFillCtx, x: u32, y: u32, cov: f32) {
    let alpha = f32_to_u32_clamped((f32::from(ctx.paint.color.a) * cov).round());
    match ctx.const_src {
        Some(src) => {
            if !ctx.clip.contains(x as i32, y as i32) {
                return;
            }
            let idx = (y * bw + x) as usize;
            if idx >= buf.len() {
                return;
            }
            buf[idx] = src.over_at_alpha(buf[idx], alpha);
        }
        None => {
            let c = pixel::color_to_u32_with_coverage(ctx.paint.color, cov);
            pixel::put(buf, bw, ctx.clip, x as i32, y as i32, c, ctx.mode);
        }
    }
}

fn fill_rect_edge_span(
    buf: &mut Band<'_>,
    bw: u32,
    ctx: &RectFillCtx,
    xa: u32,
    xb: u32,
    y: u32,
    row_cov: f32,
) {
    for x in xa..xb {
        let cov = rect_cov_x(x, ctx.xl, ctx.xr) * row_cov;
        if cov <= 0.0 {
            continue;
        }
        blend_at(buf, bw, ctx, x, y, cov);
    }
}

#[allow(clippy::too_many_arguments)]
fn stroke_rect_region(
    rect: &Rect,
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
    let outer = Rect::new(
        rect.x - half,
        rect.y - half,
        rect.width + 2.0 * half,
        rect.height + 2.0 * half,
    );
    let inner = Rect::new(
        rect.x + half,
        rect.y + half,
        (rect.width - 2.0 * half).max(0.0),
        (rect.height - 2.0 * half).max(0.0),
    );
    if inner.width <= 0.0 || inner.height <= 0.0 {
        let fill = paint.clone().style(PaintStyle::Fill);
        return fill_rect_region(&outer, &fill, mode, buf, bw, clip, x0, y0, x1, y1);
    }
    let const_src =
        (mode == BlendMode::Normal).then(|| ConstSrc::new(pixel::color_to_u32(paint.color)));
    let ctx = RectFillCtx {
        clip,
        paint,
        mode,
        xl: outer.x,
        xr: outer.right(),
        c_full: pixel::color_to_u32(paint.color),
        opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
        const_src,
    };
    let band = |from: f32, to: f32| {
        f32_to_u32_clamped(from.floor()).clamp(x0, x1)..f32_to_u32_clamped(to.ceil()).clamp(x0, x1)
    };
    let left = band(outer.x, inner.x);
    let right = band(inner.right(), outer.right());
    for y in y0..y1 {
        let out_y = rect_cov_y(y, outer.y, outer.bottom());
        if out_y <= 0.0 {
            continue;
        }
        let in_y = rect_cov_y(y, inner.y, inner.bottom());
        let cov_at = |x: u32| {
            out_y * rect_cov_x(x, outer.x, outer.right())
                - in_y * rect_cov_x(x, inner.x, inner.right())
        };
        let stroke_at = |buf: &mut Band<'_>, x: u32| {
            let cov = cov_at(x);
            if cov <= 0.0 {
                return;
            }
            if cov >= 1.0 && ctx.opaque_fast {
                pixel::put(buf, bw, clip, x as i32, y as i32, ctx.c_full, mode);
            } else {
                blend_at(buf, bw, &ctx, x, y, cov.min(1.0));
            }
        };
        if in_y >= 1.0 {
            for x in left.clone().chain(right.clone()) {
                stroke_at(buf, x);
            }
            continue;
        }
        // Only the columns holding an outer or inner edge can have a coverage that differs
        // from their neighbours, so each run between them is one span.
        let mut edges = [u32::MAX; 4];
        for (slot, e) in edges
            .iter_mut()
            .zip([outer.x, outer.right(), inner.x, inner.right()])
        {
            let f = e.floor();
            if f >= x0 as f32 && f < x1 as f32 {
                *slot = f32_to_u32_clamped(f);
            }
        }
        edges.sort_unstable();
        let run = |buf: &mut Band<'_>, from: u32, to: u32| {
            if from < to {
                let cov = cov_at(from);
                if cov > 0.0 {
                    blend_span_at(buf, bw, &ctx, from, to, y, cov.min(1.0));
                }
            }
        };
        let mut from = x0;
        for e in edges {
            if e >= x1 || e < from {
                continue;
            }
            run(buf, from, e);
            stroke_at(buf, e);
            from = e + 1;
        }
        run(buf, from, x1);
    }
}

fn on_pixel_grid(rect: &Rect) -> bool {
    [rect.x, rect.y, rect.right(), rect.bottom()]
        .iter()
        .all(|v| (v - v.round()).abs() < 1e-3)
}

fn rect_cov_x(x: u32, xl: f32, xr: f32) -> f32 {
    ((x as f32 + 1.0).min(xr) - (x as f32).max(xl)).clamp(0.0, 1.0)
}

fn rect_cov_y(y: u32, yl: f32, yr: f32) -> f32 {
    ((y as f32 + 1.0).min(yr) - (y as f32).max(yl)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod outline {

    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;

    use crate::types::{BlendMode, Color, Paint, PaintStyle, Rect};

    fn stroke_split(split: u32) -> Vec<u32> {
        let (w, h) = (20, 10);
        let mut buf = vec![0u32; (w * h) as usize];
        let rect = Rect::new(2.0, 2.0, 16.0, 6.0);
        let paint = Paint::new()
            .color(Color::rgb(255, 0, 0))
            .style(PaintStyle::Stroke)
            .stroke_width(1.0);
        for clip in [
            ClipBox {
                x0: 0,
                y0: 0,
                x1: split,
                y1: h,
            },
            ClipBox {
                x0: split,
                y0: 0,
                x1: w,
                y1: h,
            },
        ] {
            super::draw(
                &rect,
                &paint,
                BlendMode::Normal,
                &mut Band::whole(&mut buf),
                w,
                clip,
            );
        }
        buf
    }

    #[test]
    fn a_split_rectangle_has_no_edge_at_the_split() {
        let buf = stroke_split(10);
        let at = |x: u32, y: u32| buf[(y * 20 + x) as usize];
        assert_eq!(at(9, 4), 0);
        assert_eq!(at(10, 4), 0);
        assert_ne!(at(2, 4), 0);
        assert_ne!(at(17, 4), 0);
        assert_ne!(at(9, 2), 0);
        assert_ne!(at(10, 7), 0);
    }

    #[test]
    fn splitting_does_not_change_the_outline() {
        assert_eq!(stroke_split(10), stroke_split(20));
    }
}

#[cfg(test)]
mod spans {
    //! Oracles are copies of the pre-change code. The tests compare whole buffers against them.

    use super::*;
    use crate::cpu::blend::ConstSrc;
    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;

    use crate::cpu::raster::pixel;
    use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped};
    use crate::types::{BlendMode, Color, Paint, PaintStyle, Rect};

    #[allow(clippy::too_many_arguments)]
    fn ref_draw_rect(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let reach = match paint.style {
            PaintStyle::Fill => 0.0,
            PaintStyle::Stroke => paint.stroke_width / 2.0,
        };
        let x0 = f32_to_u32_clamped((rect.x - reach).floor().max(clip.left())).min(clip.x1);
        let y0 = f32_to_u32_clamped((rect.y - reach).floor().max(clip.top())).min(clip.y1);
        let x1 =
            f32_to_u32_clamped((rect.x + rect.width + reach).ceil().max(clip.left())).min(clip.x1);
        let y1 =
            f32_to_u32_clamped((rect.y + rect.height + reach).ceil().max(clip.top())).min(clip.y1);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        match paint.style {
            PaintStyle::Fill => {
                ref_fill_rect_region(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1)
            }
            PaintStyle::Stroke => {
                ref_stroke_rect_region(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ref_fill_rect_region(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        if paint.color.a == 255 && mode == BlendMode::Normal && super::on_pixel_grid(rect) {
            let c = pixel::color_to_u32(paint.color);
            for y in y0..y1 {
                let start = (y * bw + x0) as usize;
                let end = (y * bw + x1) as usize;
                if end <= buf.len() {
                    buf[start..end].fill(c);
                }
            }
        } else {
            ref_fill_rect_region_translucent(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ref_fill_rect_region_translucent(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        let xl = rect.x;
        let xr = rect.x + rect.width;
        let yl = rect.y;
        let yr = rect.y + rect.height;

        let xi0 = f32_to_i32_clamped(xl.ceil())
            .clamp(x0 as i32, x1 as i32)
            .unsigned_abs();
        let xi1 = f32_to_i32_clamped(xr.floor())
            .clamp(x0 as i32, x1 as i32)
            .unsigned_abs();
        let has_full_x = xi0 < xi1;
        let yi0 = f32_to_i32_clamped(yl.ceil())
            .clamp(y0 as i32, y1 as i32)
            .unsigned_abs();
        let yi1 = f32_to_i32_clamped(yr.floor())
            .clamp(y0 as i32, y1 as i32)
            .unsigned_abs();

        let const_src =
            (mode == BlendMode::Normal).then(|| ConstSrc::new(pixel::color_to_u32(paint.color)));
        let ctx = RectFillCtx {
            clip,
            paint,
            mode,
            xl,
            xr,
            c_full: pixel::color_to_u32(paint.color),
            opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
            const_src,
        };

        for y in y0..y1 {
            let row_cov = if y >= yi0 && y < yi1 {
                1.0
            } else {
                super::rect_cov_y(y, yl, yr)
            };
            if row_cov <= 0.0 {
                continue;
            }
            ref_fill_rect_row_translucent(buf, bw, &ctx, x0, x1, xi0, xi1, has_full_x, y, row_cov);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ref_fill_rect_row_translucent(
        buf: &mut [u32],
        bw: u32,
        ctx: &RectFillCtx,
        x0: u32,
        x1: u32,
        xi0: u32,
        xi1: u32,
        has_full_x: bool,
        y: u32,
        row_cov: f32,
    ) {
        if !has_full_x {
            super::fill_rect_edge_span(&mut Band::whole(buf), bw, ctx, x0, x1, y, row_cov);
            return;
        }

        super::fill_rect_edge_span(&mut Band::whole(buf), bw, ctx, x0, xi0, y, row_cov);

        if row_cov >= 1.0 {
            if ctx.opaque_fast {
                let row_start = (y * bw) as usize;
                buf[row_start + xi0 as usize..row_start + xi1 as usize].fill(ctx.c_full);
            } else if let Some(src) = ctx.const_src {
                let row_start = (y * bw) as usize;
                let span = row_start + xi0 as usize..row_start + xi1 as usize;
                if let Some(pixels) = buf.get_mut(span) {
                    src.over_span(pixels);
                }
            } else {
                for x in xi0..xi1 {
                    pixel::put(
                        &mut Band::whole(buf),
                        bw,
                        ctx.clip,
                        x as i32,
                        y as i32,
                        ctx.c_full,
                        ctx.mode,
                    );
                }
            }
        } else {
            for x in xi0..xi1 {
                super::blend_at(&mut Band::whole(buf), bw, ctx, x, y, row_cov);
            }
        }

        super::fill_rect_edge_span(&mut Band::whole(buf), bw, ctx, xi1, x1, y, row_cov);
    }

    #[allow(clippy::too_many_arguments)]
    fn ref_stroke_rect_region(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
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
        let outer = Rect::new(
            rect.x - half,
            rect.y - half,
            rect.width + 2.0 * half,
            rect.height + 2.0 * half,
        );
        let inner = Rect::new(
            rect.x + half,
            rect.y + half,
            (rect.width - 2.0 * half).max(0.0),
            (rect.height - 2.0 * half).max(0.0),
        );
        if inner.width <= 0.0 || inner.height <= 0.0 {
            let fill = paint.clone().style(PaintStyle::Fill);
            return ref_fill_rect_region(&outer, &fill, mode, buf, bw, clip, x0, y0, x1, y1);
        }
        let const_src =
            (mode == BlendMode::Normal).then(|| ConstSrc::new(pixel::color_to_u32(paint.color)));
        let ctx = RectFillCtx {
            clip,
            paint,
            mode,
            xl: outer.x,
            xr: outer.right(),
            c_full: pixel::color_to_u32(paint.color),
            opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
            const_src,
        };
        let band = |from: f32, to: f32| {
            f32_to_u32_clamped(from.floor()).clamp(x0, x1)
                ..f32_to_u32_clamped(to.ceil()).clamp(x0, x1)
        };
        let left = band(outer.x, inner.x);
        let right = band(inner.right(), outer.right());
        for y in y0..y1 {
            let out_y = super::rect_cov_y(y, outer.y, outer.bottom());
            if out_y <= 0.0 {
                continue;
            }
            let in_y = super::rect_cov_y(y, inner.y, inner.bottom());
            let mut stroke_at = |x: u32| {
                let cov = out_y * super::rect_cov_x(x, outer.x, outer.right())
                    - in_y * super::rect_cov_x(x, inner.x, inner.right());
                if cov <= 0.0 {
                    return;
                }
                if cov >= 1.0 && ctx.opaque_fast {
                    pixel::put(
                        &mut Band::whole(buf),
                        bw,
                        clip,
                        x as i32,
                        y as i32,
                        ctx.c_full,
                        mode,
                    );
                } else {
                    super::blend_at(&mut Band::whole(buf), bw, &ctx, x, y, cov.min(1.0));
                }
            };
            if in_y >= 1.0 {
                left.clone().for_each(&mut stroke_at);
                right.clone().for_each(&mut stroke_at);
            } else {
                (x0..x1).for_each(&mut stroke_at);
            }
        }
    }

    fn dst_uniform(w: u32, h: u32) -> Vec<u32> {
        vec![0xff10_2030; (w * h) as usize]
    }

    /// Varied colours and alphas, including fully transparent pixels.
    fn dst_varied(w: u32, h: u32) -> Vec<u32> {
        (0..w * h)
            .map(|i| {
                if i % 5 == 0 {
                    0
                } else {
                    i.wrapping_mul(2_654_435_761)
                }
            })
            .collect()
    }

    const FRACTIONAL_RECTS: [(f32, f32, f32, f32); 8] = [
        (10.3, 5.6, 40.25, 20.4),
        (3.0, 2.5, 30.0, 7.75),
        (-4.5, 1.25, 20.5, 30.5),
        (50.2, 40.7, 30.0, 20.0),
        (0.0, 0.0, 64.0, 48.0),
        (12.5, 0.4, 0.5, 30.0),
        (-100.0, -100.0, 10.0, 10.0),
        (7.9, 3.33, 45.6, 0.9),
    ];

    fn colours() -> [Color; 4] {
        [
            Color::rgba(40, 120, 200, 128),
            Color::rgba(200, 30, 90, 255),
            Color::rgba(10, 10, 10, 90),
            Color::rgb(255, 255, 255),
        ]
    }

    fn clips() -> [ClipBox; 3] {
        [
            ClipBox::surface(64, 48),
            ClipBox {
                x0: 5,
                y0: 4,
                x1: 50,
                y1: 40,
            },
            ClipBox {
                x0: 0,
                y0: 0,
                x1: 20,
                y1: 48,
            },
        ]
    }

    #[test]
    fn fill_rect_matches_reference_over_fractional_rects() {
        const W: u32 = 64;
        const H: u32 = 48;
        let mut cases = 0;
        for (x, y, w, h) in FRACTIONAL_RECTS {
            let rect = Rect::new(x, y, w, h);
            for color in colours() {
                let paint = Paint::new().color(color);
                for mode in [BlendMode::Normal, BlendMode::Multiply] {
                    for clip in clips() {
                        for dst in [dst_uniform(W, H), dst_varied(W, H)] {
                            let mut new = dst.clone();
                            let mut old = dst;
                            super::draw(&rect, &paint, mode, &mut Band::whole(&mut new), W, clip);
                            ref_draw_rect(&rect, &paint, mode, &mut old, W, clip);
                            assert_eq!(
                                new, old,
                                "rect {rect:?} colour {color:?} mode {mode:?} clip {clip:?}"
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 8 * 4 * 2 * 3 * 2);
    }

    #[test]
    fn stroke_rect_matches_reference_over_fractional_rects() {
        const W: u32 = 64;
        const H: u32 = 48;
        for (x, y, w, h) in FRACTIONAL_RECTS {
            let rect = Rect::new(x, y, w, h);
            for color in colours() {
                for width in [1.0, 2.5, 6.0, 40.0] {
                    let paint = Paint::new()
                        .color(color)
                        .style(PaintStyle::Stroke)
                        .stroke_width(width);
                    for mode in [BlendMode::Normal, BlendMode::Multiply] {
                        for clip in clips() {
                            for dst in [dst_uniform(W, H), dst_varied(W, H)] {
                                let mut new = dst.clone();
                                let mut old = dst;
                                super::draw(
                                    &rect,
                                    &paint,
                                    mode,
                                    &mut Band::whole(&mut new),
                                    W,
                                    clip,
                                );
                                ref_draw_rect(&rect, &paint, mode, &mut old, W, clip);
                                assert_eq!(
                                    new, old,
                                    "rect {rect:?} colour {color:?} width {width} mode {mode:?} clip {clip:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
