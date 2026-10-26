//! Rounded rectangles, filled or stroked, antialiased analytically.

use std::f32::consts::SQRT_2;
use std::ops::Range;

use crate::cpu::blend::{ConstSrc, blend_pixel};
use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped};
use crate::types::{BlendMode, Color, Paint, PaintStyle, Rect};

pub(crate) fn draw(
    rect: Rect,
    radius: f32,
    paint: &Paint,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
) {
    if clip.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 || paint.color.a == 0 {
        return;
    }
    let shape = Shape::new(rect, radius);
    let mut px = Pixels::new(buf, bw, clip, paint.color, mode);
    match paint.style {
        PaintStyle::Fill => fill(&shape, &mut px),
        PaintStyle::Stroke if paint.stroke_width > 0.0 => {
            stroke(&shape, paint.stroke_width / 2.0, &mut px);
        }
        PaintStyle::Stroke => {}
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn shadow(
    rect: Rect,
    radius: f32,
    blur: f32,
    color: Color,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
) {
    if clip.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 || color.a == 0 {
        return;
    }
    let shape = Shape::new(rect, radius);
    let mut px = Pixels::new(buf, bw, clip, color, mode);
    if blur <= 0.0 {
        fill(&shape, &mut px);
        return;
    }
    let reach = blur * 3.0;
    for y in rows(&shape, reach, px.clip) {
        let Some(outer) = shape.outer_half(shape.row_q(y as f32 + 0.5), reach) else {
            continue;
        };
        let span = columns(&shape, outer, px.clip, false);
        let solid = deep_run(&shape, y, reach, px.clip, true);
        row(&shape, y, span, solid, &mut px, |d| {
            shadow_coverage(d, blur)
        });
    }
}

pub(crate) fn shadow_coverage(d: f32, blur: f32) -> f32 {
    let x = d.abs() / (blur * SQRT_2);
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_6
            + t * (-0.284_496_74 + t * (1.421_413_7 + t * (-1.453_152 + t * 1.061_405_4))));
    let tail = 0.5 * poly * (-x * x).exp();
    if d < 0.0 { 1.0 - tail } else { tail }
}

#[derive(Clone, Copy, Debug)]
struct Shape {
    cx: f32,
    cy: f32,
    hx: f32,
    hy: f32,
    r: f32,
}

impl Shape {
    fn new(rect: Rect, radius: f32) -> Self {
        let (hx, hy) = (rect.width / 2.0, rect.height / 2.0);
        Self {
            cx: rect.x + hx,
            cy: rect.y + hy,
            hx,
            hy,
            r: radius.max(0.0).min(hx).min(hy),
        }
    }

    fn distance(&self, x: f32, y: f32) -> f32 {
        let qx = (x - self.cx).abs() - (self.hx - self.r);
        let qy = (y - self.cy).abs() - (self.hy - self.r);
        let (ox, oy) = (qx.max(0.0), qy.max(0.0));
        let outside = (ox * ox + oy * oy).sqrt();
        let inside = qx.max(qy).min(0.0);
        outside + inside - self.r
    }

    fn row_q(&self, y: f32) -> f32 {
        (y - self.cy).abs() - (self.hy - self.r)
    }

    fn outer_half(&self, qy: f32, k: f32) -> Option<f32> {
        let qy = qy.max(0.0);
        let reach = self.r + k;
        if reach <= 0.0 || qy >= reach {
            return None;
        }
        Some(self.hx - self.r + (reach * reach - qy * qy).sqrt())
    }

    fn inner_half(&self, qy: f32, k: f32) -> Option<f32> {
        if qy <= 0.0 {
            return (qy <= self.r - k && self.hx > k).then_some(self.hx - k);
        }
        let reach = self.r - k;
        if reach <= 0.0 || qy >= reach {
            return None;
        }
        Some(self.hx - self.r + (reach * reach - qy * qy).sqrt())
    }
}

fn fill_coverage(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

fn stroke_coverage(d: f32, half: f32) -> f32 {
    let t = d.abs();
    ((t + 0.5).min(half) - (t - 0.5).max(-half)).clamp(0.0, 1.0)
}

fn rows(shape: &Shape, k: f32, clip: ClipBox) -> Range<u32> {
    let top = f32_to_u32_clamped((shape.cy - shape.hy - k).floor()).max(clip.y0);
    let bottom = f32_to_u32_clamped((shape.cy + shape.hy + k).ceil()).min(clip.y1);
    top..bottom.max(top)
}

fn columns(shape: &Shape, half: f32, clip: ClipBox, fully: bool) -> (i32, i32) {
    let (lo, hi) = if fully {
        (
            (shape.cx - half - 0.5).ceil(),
            (shape.cx + half - 0.5).floor() + 1.0,
        )
    } else {
        (
            (shape.cx - half - 0.5).floor(),
            (shape.cx + half + 0.5).ceil(),
        )
    };
    let left = i32::try_from(clip.x0).unwrap_or(i32::MAX);
    let right = i32::try_from(clip.x1).unwrap_or(i32::MAX);
    let from = f32_to_i32_clamped(lo).clamp(left, right);
    let to = f32_to_i32_clamped(hi).clamp(left, right);
    (from, to.max(from))
}

#[derive(Clone, Copy)]
struct Run {
    from: i32,
    to: i32,
    solid: bool,
}

fn row(
    shape: &Shape,
    y: u32,
    (xa, xb): (i32, i32),
    known: Run,
    px: &mut Pixels<'_, '_>,
    cover: impl Fn(f32) -> f32,
) {
    let cy = y as f32 + 0.5;
    let qy = shape.row_q(cy);
    let mut segments = [(xa, xb), (xb, xb)];
    if qy > 0.0 {
        let (m0, m1) = columns(shape, shape.hx - shape.r, px.clip, true);
        let (m0, m1) = (m0.clamp(xa, xb), m1.clamp(xa, xb).max(m0.clamp(xa, xb)));
        px.span_at(y, m0, m1, cover(qy - shape.r));
        segments = [(xa, m0), (m1, xb)];
    }
    for (lo, hi) in segments {
        let s0 = known.from.clamp(lo, hi);
        let s1 = known.to.clamp(lo, hi).max(s0);
        for x in (lo..s0).chain(s1..hi) {
            px.put(x, y, cover(shape.distance(x as f32 + 0.5, cy)));
        }
        if known.solid {
            px.span_at(y, s0, s1, 1.0);
        }
    }
}

fn deep_run(shape: &Shape, y: u32, k: f32, clip: ClipBox, solid: bool) -> Run {
    let qy = shape.row_q(y as f32 + 0.5);
    match shape.inner_half(qy, k) {
        Some(inner) => {
            let (from, to) = columns(shape, inner, clip, true);
            Run { from, to, solid }
        }
        None => Run {
            from: 0,
            to: 0,
            solid,
        },
    }
}

fn fill(shape: &Shape, px: &mut Pixels<'_, '_>) {
    for y in rows(shape, 0.5, px.clip) {
        let Some(outer) = shape.outer_half(shape.row_q(y as f32 + 0.5), 0.5) else {
            continue;
        };
        let span = columns(shape, outer, px.clip, false);
        let solid = deep_run(shape, y, 0.5, px.clip, true);
        row(shape, y, span, solid, px, fill_coverage);
    }
}

fn stroke(shape: &Shape, half: f32, px: &mut Pixels<'_, '_>) {
    let reach = half + 0.5;
    for y in rows(shape, reach, px.clip) {
        let Some(outer) = shape.outer_half(shape.row_q(y as f32 + 0.5), reach) else {
            continue;
        };
        let span = columns(shape, outer, px.clip, false);
        let hole = deep_run(shape, y, reach, px.clip, false);
        row(shape, y, span, hole, px, |d| stroke_coverage(d, half));
    }
}

struct Pixels<'a, 'b> {
    buf: &'a mut Band<'b>,
    bw: u32,
    clip: ClipBox,
    color: Color,
    mode: BlendMode,
    full: u32,
    src: Option<ConstSrc>,
}

impl<'a, 'b> Pixels<'a, 'b> {
    fn new(buf: &'a mut Band<'b>, bw: u32, clip: ClipBox, color: Color, mode: BlendMode) -> Self {
        let full = (u32::from(color.a) << 24)
            | (u32::from(color.r) << 16)
            | (u32::from(color.g) << 8)
            | u32::from(color.b);
        Self {
            buf,
            bw,
            clip,
            color,
            mode,
            full,
            src: (mode == BlendMode::Normal).then(|| ConstSrc::new(full)),
        }
    }

    fn index(&self, x: i32, y: u32) -> Option<usize> {
        if !self.clip.contains(x, i32::try_from(y).ok()?) {
            return None;
        }
        let idx = usize::try_from(y * self.bw + x.unsigned_abs()).ok()?;
        (idx < self.buf.len()).then_some(idx)
    }

    fn put(&mut self, x: i32, y: u32, cov: f32) {
        if cov <= 0.0 {
            return;
        }
        let Some(idx) = self.index(x, y) else {
            return;
        };
        let alpha = f32_to_u32_clamped((f32::from(self.color.a) * cov).round());
        let dst = self.buf[idx];
        self.buf[idx] = match self.src {
            Some(_) if alpha == 255 => self.full,
            Some(src) => src.over_at_alpha(dst, alpha),
            None => {
                let c = (alpha << 24) | (self.full & 0x00FF_FFFF);
                blend_pixel(c, dst, self.mode)
            }
        };
    }

    fn span_at(&mut self, y: u32, xa: i32, xb: i32, cov: f32) {
        if xa >= xb || cov <= 0.0 {
            return;
        }
        let (Some(start), Some(last)) = (self.index(xa, y), self.index(xb - 1, y)) else {
            for x in xa..xb {
                self.put(x, y, cov);
            }
            return;
        };
        let alpha = f32_to_u32_clamped((f32::from(self.color.a) * cov.min(1.0)).round());
        let row = &mut self.buf[start..=last];
        match self.src {
            Some(_) if alpha == 255 => row.fill(self.full),
            Some(src) => src.over_span_at_alpha(row, alpha),
            None => {
                let c = (alpha << 24) | (self.full & 0x00FF_FFFF);
                for p in row {
                    *p = blend_pixel(c, *p, self.mode);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;

    const W: u32 = 64;

    fn canvas() -> Vec<u32> {
        vec![0; (W * W) as usize]
    }

    fn alpha(buf: &[u32], x: u32, y: u32) -> u32 {
        buf[(y * W + x) as usize] >> 24
    }

    fn paint(color: Color, style: PaintStyle, width: f32) -> Paint {
        Paint::new().color(color).style(style).stroke_width(width)
    }

    #[test]
    fn shadow_coverage_is_a_gaussian_edge() {
        assert!((shadow_coverage(0.0, 4.0) - 0.5).abs() < 1e-3);
        for d in [0.5, 2.0, 5.0, 9.0] {
            let sum = shadow_coverage(d, 4.0) + shadow_coverage(-d, 4.0);
            assert!((sum - 1.0).abs() < 1e-4, "{d}: {sum}");
        }
        assert!(shadow_coverage(12.0, 4.0) < 0.002);
        assert!(shadow_coverage(-12.0, 4.0) > 0.998);
    }

    #[test]
    fn a_shadow_ramps_without_steps() {
        let mut buf = canvas();
        shadow(
            Rect::new(20.0, 16.0, 24.0, 32.0),
            6.0,
            4.0,
            Color::rgb(0, 0, 0),
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            W,
            ClipBox::surface(W, W),
        );
        let row: Vec<u32> = (0..W).map(|x| alpha(&buf, x, 32)).collect();
        assert_eq!(row[2], 0, "{row:?}");
        assert!(row[32] > 250, "{row:?}");
        assert!(
            row[19].abs_diff(128) < 24 && row[20].abs_diff(128) < 24,
            "{row:?}"
        );
        for x in 1..32 {
            assert!(row[x] >= row[x - 1], "not rising at {x}: {row:?}");
            assert!(row[x] - row[x - 1] < 40, "hard step at {x}: {row:?}");
        }
    }

    #[test]
    fn a_fill_is_solid_inside_and_empty_past_its_corners() {
        let mut buf = canvas();
        let p = paint(Color::rgb(255, 255, 255), PaintStyle::Fill, 0.0);
        draw(
            Rect::new(8.0, 8.0, 40.0, 24.0),
            10.0,
            &p,
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            W,
            ClipBox::surface(W, W),
        );
        assert_eq!(alpha(&buf, 28, 20), 255, "middle");
        assert_eq!(alpha(&buf, 8, 20), 255, "left edge between the corners");
        assert_eq!(alpha(&buf, 8, 8), 0, "the corner is cut away");
        assert_eq!(alpha(&buf, 7, 20), 0, "outside");
        let arc = alpha(&buf, 8, 14);
        assert!(arc > 0 && arc < 255, "corner arc is antialiased, got {arc}");
    }

    #[test]
    fn coverage_adds_up_to_the_area() {
        let mut buf = canvas();
        let p = paint(Color::rgb(255, 255, 255), PaintStyle::Fill, 0.0);
        let (w, h, r) = (30.5_f32, 20.25_f32, 6.0_f32);
        draw(
            Rect::new(10.3, 12.6, w, h),
            r,
            &p,
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            W,
            ClipBox::surface(W, W),
        );
        let total: f32 = buf.iter().map(|px| (px >> 24) as f32 / 255.0).sum();
        let area = w * h - (4.0 - PI) * r * r;
        assert!((total - area).abs() < area * 0.01, "{total} vs {area}");
    }

    #[test]
    fn a_large_radius_makes_a_pill() {
        let mut buf = canvas();
        let p = paint(Color::rgb(255, 255, 255), PaintStyle::Fill, 0.0);
        draw(
            Rect::new(4.0, 4.0, 40.0, 16.0),
            100.0,
            &p,
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            W,
            ClipBox::surface(W, W),
        );
        assert!(
            alpha(&buf, 4, 12) > 240,
            "the round end reaches the left edge"
        );
        assert_eq!(alpha(&buf, 3, 12), 0);
        assert!(alpha(&buf, 5, 5) < 128, "and curves away above it");
        assert_eq!(alpha(&buf, 24, 4), 255, "flat top");
        assert_eq!(alpha(&buf, 4, 4), 0);
    }

    #[test]
    fn a_stroke_draws_the_outline_and_leaves_the_middle() {
        let mut buf = canvas();
        let p = paint(Color::rgb(255, 255, 255), PaintStyle::Stroke, 2.0);
        draw(
            Rect::new(10.0, 10.0, 40.0, 30.0),
            6.0,
            &p,
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            W,
            ClipBox::surface(W, W),
        );
        assert_eq!(alpha(&buf, 30, 25), 0, "middle stays empty");
        assert_eq!(alpha(&buf, 9, 25), 255);
        assert_eq!(alpha(&buf, 10, 25), 255);
        assert_eq!(alpha(&buf, 8, 25), 0);
        assert_eq!(alpha(&buf, 11, 25), 0);
    }

    #[test]
    fn a_hairline_fades_instead_of_vanishing() {
        let mut buf = canvas();
        let p = paint(Color::rgb(255, 255, 255), PaintStyle::Stroke, 0.5);
        draw(
            Rect::new(10.5, 10.5, 40.0, 30.0),
            0.0,
            &p,
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            W,
            ClipBox::surface(W, W),
        );
        let a = alpha(&buf, 10, 25);
        assert!(
            (120..=135).contains(&a),
            "half a pixel wide is half covered, got {a}"
        );
    }

    #[test]
    fn the_clip_is_kept() {
        let mut buf = canvas();
        let p = paint(Color::rgb(255, 255, 255), PaintStyle::Fill, 0.0);
        let clip = ClipBox::surface(W, W).intersect(Rect::new(0.0, 0.0, 20.0, 64.0));
        draw(
            Rect::new(8.0, 8.0, 40.0, 24.0),
            4.0,
            &p,
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            W,
            clip,
        );
        assert_eq!(alpha(&buf, 19, 20), 255);
        assert_eq!(alpha(&buf, 20, 20), 0);
    }

    #[test]
    fn translucent_fills_blend() {
        let mut buf = vec![0xFF00_0000; (W * W) as usize];
        let p = paint(Color::rgba(255, 255, 255, 128), PaintStyle::Fill, 0.0);
        draw(
            Rect::new(8.0, 8.0, 40.0, 24.0),
            4.0,
            &p,
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            W,
            ClipBox::surface(W, W),
        );
        let px = buf[(20 * W + 28) as usize];
        let r = (px >> 16) & 0xFF;
        assert!(
            r > 100 && r < 255,
            "white over black at half alpha, got {r}"
        );
        assert_eq!(px >> 24, 255);
    }
}
