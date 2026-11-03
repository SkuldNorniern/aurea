//! Gradients: a 256 step table per draw, walked per row or per pixel.

use crate::cpu::blend::{LinearLuts, blend_pixel};
use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::cpu::raster::pixel;
use crate::numeric::{f32_to_i32_clamped, f32_to_u8_clamped, f32_to_usize_clamped};
use crate::types::{BlendMode, GradientStop, LinearGradient, Point, RadialGradient, Rect};
use std::cell::RefCell;

/// Frees this thread's ramp buffer.
pub(crate) fn trim_scratch() {
    RAMP_INDICES.with_borrow_mut(|indices| *indices = Vec::new());
}

thread_local! {
    /// Lookup index per column of the horizontal gradient ramp, reused by every span on this thread.
    pub(crate) static RAMP_INDICES: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// A gradient LUT colour, with the parts a Normal fill over an opaque pixel reads precomputed.
#[derive(Clone, Copy)]
struct GradientStep {
    color: u32,
    alpha: u8,
    /// Red, green and blue in 12-bit linear light, as [`LinearLuts`] stores them.
    linear: [u32; 3],
}

/// A linear gradient's axis in pixel space, as `fill_linear_gradient` uses it.
struct LinearRamp {
    start: Point,
    dx: f32,
    dy: f32,
    len_sq: f32,
}

impl LinearRamp {
    /// Position along the ramp at the pixel whose top-left corner is `(cx, cy)`.
    fn t_at(&self, cx: f32, cy: f32) -> f32 {
        ((cx + 0.5 - self.start.x) * self.dx + (cy + 0.5 - self.start.y) * self.dy) / self.len_sq
    }
}

fn color_at(stops: &[GradientStop], t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    if stops.is_empty() {
        return 0;
    }
    if stops.len() == 1 {
        let c = stops[0].color;
        return (u32::from(c.a) << 24)
            | (u32::from(c.r) << 16)
            | (u32::from(c.g) << 8)
            | u32::from(c.b);
    }
    for w in stops.windows(2) {
        let (a, b) = (w[0].offset, w[1].offset);
        if t >= a && t <= b {
            let s = if (b - a).abs() < 1e-6 {
                1.0
            } else {
                (t - a) / (b - a)
            };
            let (c0, c1) = (w[0].color, w[1].color);
            let lerp = |a: u8, b: u8| {
                f32_to_u8_clamped((f32::from(a) + (f32::from(b) - f32::from(a)) * s).round())
            };
            let (r, g, b_, a_) = (
                lerp(c0.r, c1.r),
                lerp(c0.g, c1.g),
                lerp(c0.b, c1.b),
                lerp(c0.a, c1.a),
            );
            return (u32::from(a_) << 24)
                | (u32::from(r) << 16)
                | (u32::from(g) << 8)
                | u32::from(b_);
        }
    }
    let c = if t <= stops[0].offset {
        stops[0].color
    } else {
        stops.last().expect("stops has at least 2 elements").color
    };
    (u32::from(c.a) << 24) | (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
}

pub(crate) fn build_lut(stops: &[GradientStop]) -> [u32; 256] {
    let mut lut = [0u32; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        *slot = color_at(stops, i as f32 / 255.0);
    }
    lut
}

pub(crate) fn fill_linear(
    grad: &LinearGradient,
    rect: Rect,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
) {
    let (start, end) = grad.pixel_axis();
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-10 {
        return;
    }
    let lut = build_lut(&grad.stops);
    let x0 = f32_to_i32_clamped(rect.x.max(clip.left()).ceil());
    let y0 = f32_to_i32_clamped(rect.y.max(clip.top()).ceil());
    let x1 = f32_to_i32_clamped((rect.x + rect.width).min(clip.right()).floor());
    let y1 = f32_to_i32_clamped((rect.y + rect.height).min(clip.bottom()).floor());
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let ramp = LinearRamp {
        start,
        dx,
        dy,
        len_sq,
    };
    let (cols, rows) = ((x0, x1), (y0, y1));
    match mode {
        BlendMode::Normal if dx == 0.0 => {
            fill_vertical_ramp(&ramp, &lut, cols, rows, bw, buf);
        }
        BlendMode::Normal if dy == 0.0 => {
            fill_horizontal_ramp(&ramp, &lut, cols, rows, bw, buf);
        }
        _ => fill_linear_general(&ramp, &lut, cols, rows, mode, bw, buf),
    }
}

/// Vertical ramp: `t` is the same across a row, so each row is one span of one step.
fn fill_vertical_ramp(
    ramp: &LinearRamp,
    lut: &[u32; 256],
    cols: (i32, i32),
    rows: (i32, i32),
    bw: u32,
    buf: &mut Band<'_>,
) {
    let luts = LinearLuts::get();
    let steps = gradient_steps(lut, luts);
    let (x0, x1) = cols;
    for cy in rows.0..rows.1 {
        let row = (cy.unsigned_abs() * bw) as usize;
        let step = &steps[lut_index(ramp.t_at(x0 as f32, cy as f32))];
        let from = row + x0.unsigned_abs() as usize;
        let to = row + x1.unsigned_abs() as usize;
        gradient_over_span(step, &mut buf[from..to], luts);
    }
}

/// Horizontal ramp: every row sees the same `t` per column, so the steps are found once.
fn fill_horizontal_ramp(
    ramp: &LinearRamp,
    lut: &[u32; 256],
    cols: (i32, i32),
    rows: (i32, i32),
    bw: u32,
    buf: &mut Band<'_>,
) {
    let luts = LinearLuts::get();
    let steps = gradient_steps(lut, luts);
    let (x0, x1) = cols;
    let dt_x = ramp.dx / ramp.len_sq;
    let mut t = ramp.t_at(x0 as f32, rows.0 as f32);
    RAMP_INDICES.with_borrow_mut(|indices| {
        indices.clear();
        indices.extend((x0..x1).map(|_| {
            let idx = lut_index(t);
            t += dt_x;
            idx
        }));
        for cy in rows.0..rows.1 {
            let row = (cy.unsigned_abs() * bw) as usize;
            let from = row + x0.unsigned_abs() as usize;
            for (px, &idx) in buf[from..from + indices.len()]
                .iter_mut()
                .zip(indices.iter())
            {
                *px = gradient_over(&steps[idx], *px, luts);
            }
        }
    });
}

/// Any linear ramp, or any blend mode other than Normal.
fn fill_linear_general(
    ramp: &LinearRamp,
    lut: &[u32; 256],
    cols: (i32, i32),
    rows: (i32, i32),
    mode: BlendMode,
    bw: u32,
    buf: &mut Band<'_>,
) {
    let luts = LinearLuts::get();
    let steps = (mode == BlendMode::Normal).then(|| gradient_steps(lut, luts));
    let dt_x = ramp.dx / ramp.len_sq;
    for cy in rows.0..rows.1 {
        let row = (cy.unsigned_abs() * bw) as usize;
        let mut t = ramp.t_at(cols.0 as f32, cy as f32);
        for cx in cols.0..cols.1 {
            let t_idx = lut_index(t);
            let idx = row + cx.unsigned_abs() as usize;
            buf[idx] = match &steps {
                Some(steps) => gradient_over(&steps[t_idx], buf[idx], luts),
                None => blend_pixel(lut[t_idx], buf[idx], mode),
            };
            t += dt_x;
        }
    }
}

/// Smallest squared distance from the centre that reaches each step, for `fill_radial_gradient`.
/// Entry `j` is the threshold for step `j + 1`. Found by bisection with the per-pixel
/// `sqrt`-and-round arithmetic, so a lookup gives the same step as computing it per pixel.
fn radial_thresholds(inv_radius: f32) -> [f32; 255] {
    let step_of_sq = |d2: f32| {
        let t = (d2.sqrt() * inv_radius).min(1.0);
        f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round())
    };
    let mut thresholds = [f32::INFINITY; 255];
    let largest = step_of_sq(f32::MAX);
    for (j, slot) in thresholds.iter_mut().enumerate() {
        let target = j + 1;
        if largest < target {
            continue;
        }
        let (mut lo, mut hi) = (0u32, f32::MAX.to_bits());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if step_of_sq(f32::from_bits(mid)) >= target {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        *slot = f32::from_bits(lo);
    }
    thresholds
}

pub(crate) fn fill_radial(
    grad: &RadialGradient,
    rect: Rect,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
) {
    if grad.radius <= 0.0 {
        return;
    }
    let lut = build_lut(&grad.stops);
    let x0 = f32_to_i32_clamped(rect.x.max(clip.left()).ceil());
    let y0 = f32_to_i32_clamped(rect.y.max(clip.top()).ceil());
    let x1 = f32_to_i32_clamped((rect.x + rect.width).min(clip.right()).floor());
    let y1 = f32_to_i32_clamped((rect.y + rect.height).min(clip.bottom()).floor());
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let inv_radius = 1.0 / grad.radius;
    let thresholds = radial_thresholds(inv_radius);
    let luts = LinearLuts::get();
    let steps = (mode == BlendMode::Normal).then(|| gradient_steps(&lut, luts));

    for cy in y0..y1 {
        let row = (cy.unsigned_abs() * bw) as usize;
        let dy = cy as f32 + 0.5 - grad.center.y;
        let dy_sq = dy * dy;
        // Each pixel's step is found by walking from the previous one. Squared distance is
        // monotone in the step, so the walk stops where the threshold table says it should.
        let dx0 = x0 as f32 + 0.5 - grad.center.x;
        let mut step = thresholds.partition_point(|&th| th <= dx0 * dx0 + dy_sq);
        for cx in x0..x1 {
            let dx = cx as f32 + 0.5 - grad.center.x;
            let d2 = dx * dx + dy_sq;
            while step < 255 && d2 >= thresholds[step] {
                step += 1;
            }
            while step > 0 && d2 < thresholds[step - 1] {
                step -= 1;
            }
            let idx = row + cx.unsigned_abs() as usize;
            buf[idx] = match &steps {
                Some(steps) => gradient_over(&steps[step], buf[idx], luts),
                None => blend_pixel(lut[step], buf[idx], mode),
            };
        }
    }
}

fn fade_stops(stops: &[GradientStop], alpha: f32) -> Vec<GradientStop> {
    stops
        .iter()
        .map(|stop| GradientStop {
            color: pixel::fade_color(stop.color, alpha),
            ..*stop
        })
        .collect()
}

pub(crate) fn fade_linear(grad: &LinearGradient, alpha: f32) -> LinearGradient {
    LinearGradient {
        start: grad.start,
        end: grad.end,
        stops: fade_stops(&grad.stops, alpha),
    }
}

pub(crate) fn fade_radial(grad: &RadialGradient, alpha: f32) -> RadialGradient {
    RadialGradient {
        center: grad.center,
        radius: grad.radius,
        stops: fade_stops(&grad.stops, alpha),
    }
}

fn gradient_steps(lut: &[u32; 256], luts: LinearLuts) -> [GradientStep; 256] {
    lut.map(|color| {
        let [alpha, r, g, b] = color.to_be_bytes();
        GradientStep {
            color,
            alpha,
            linear: [luts.linear(r), luts.linear(g), luts.linear(b)],
        }
    })
}

/// Source-over of a gradient step onto `dst`. Opaque and transparent steps are exact; a translucent
/// step over an opaque pixel is mixed in 12-bit linear light, within one level of `blend_pixel`.
#[inline]
fn gradient_over(step: &GradientStep, dst: u32, luts: LinearLuts) -> u32 {
    match step.alpha {
        255 => step.color,
        0 => dst,
        alpha if dst >> 24 == 255 => {
            let [_, dr, dg, db] = dst.to_be_bytes();
            let [r, g, b] = step.linear;
            0xff00_0000
                | (luts.mix(r, dr, alpha) << 16)
                | (luts.mix(g, dg, alpha) << 8)
                | luts.mix(b, db, alpha)
        }
        _ => blend_pixel(step.color, dst, BlendMode::Normal),
    }
}

/// The LUT entry for position `t`.
#[inline]
fn lut_index(t: f32) -> usize {
    f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round())
}

/// [`gradient_over`] of one step across a run of pixels. A repeated destination pixel reuses the
/// last result.
#[inline]
fn gradient_over_span(step: &GradientStep, pixels: &mut [u32], luts: LinearLuts) {
    if step.alpha == 255 {
        pixels.fill(step.color);
        return;
    }
    let Some(&first) = pixels.first() else {
        return;
    };
    let (mut last_dst, mut last_out) = (first, gradient_over(step, first, luts));
    for p in pixels {
        if *p != last_dst {
            last_dst = *p;
            last_out = gradient_over(step, last_dst, luts);
        }
        *p = last_out;
    }
}

#[cfg(test)]
mod tests {
    use crate::cpu::blend::{LinearLuts, blend_pixel};
    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;
    use crate::cpu::raster::image::ImageScratch;
    use crate::cpu::raster::{image, pixel};
    use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped, f32_to_usize_clamped};
    use crate::types::{
        BlendMode, Color, GradientStop, Image, LinearGradient, Point, RadialGradient, Rect,
        Sampling,
    };

    const W: u32 = 40;
    const H: u32 = 30;

    /// `fill_linear_gradient` as it stands before the axis-aligned paths, kept as the reference.
    fn reference_linear(
        grad: &LinearGradient,
        rect: Rect,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let (start, end) = grad.pixel_axis();
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq < 1e-10 {
            return;
        }
        let lut = super::build_lut(&grad.stops);
        let x0 = f32_to_i32_clamped(rect.x.max(clip.left()).ceil());
        let y0 = f32_to_i32_clamped(rect.y.max(clip.top()).ceil());
        let x1 = f32_to_i32_clamped((rect.x + rect.width).min(clip.right()).floor());
        let y1 = f32_to_i32_clamped((rect.y + rect.height).min(clip.bottom()).floor());
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let dt_x = dx / len_sq;
        let luts = LinearLuts::get();
        let steps = (mode == BlendMode::Normal).then(|| super::gradient_steps(&lut, luts));

        for cy in y0..y1 {
            let row = (cy.unsigned_abs() * bw) as usize;
            let mut t =
                ((x0 as f32 + 0.5 - start.x) * dx + (cy as f32 + 0.5 - start.y) * dy) / len_sq;
            for cx in x0..x1 {
                let t_idx = f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round());
                let idx = row + cx.unsigned_abs() as usize;
                buf[idx] = match &steps {
                    Some(steps) => super::gradient_over(&steps[t_idx], buf[idx], luts),
                    None => blend_pixel(lut[t_idx], buf[idx], mode),
                };
                t += dt_x;
            }
        }
    }

    /// `fill_radial_gradient` as it stands before the squared-distance path, kept as the reference.
    fn reference_radial(
        grad: &RadialGradient,
        rect: Rect,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        if grad.radius <= 0.0 {
            return;
        }
        let lut = super::build_lut(&grad.stops);
        let x0 = f32_to_i32_clamped(rect.x.max(clip.left()).ceil());
        let y0 = f32_to_i32_clamped(rect.y.max(clip.top()).ceil());
        let x1 = f32_to_i32_clamped((rect.x + rect.width).min(clip.right()).floor());
        let y1 = f32_to_i32_clamped((rect.y + rect.height).min(clip.bottom()).floor());
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let inv_radius = 1.0 / grad.radius;
        let luts = LinearLuts::get();
        let steps = (mode == BlendMode::Normal).then(|| super::gradient_steps(&lut, luts));

        for cy in y0..y1 {
            let row = (cy.unsigned_abs() * bw) as usize;
            let dy = cy as f32 + 0.5 - grad.center.y;
            let dy_sq = dy * dy;
            for cx in x0..x1 {
                let dx = cx as f32 + 0.5 - grad.center.x;
                let dist = (dx * dx + dy_sq).sqrt();
                let t = (dist * inv_radius).min(1.0);
                let t_idx = f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round());
                let idx = row + cx.unsigned_abs() as usize;
                buf[idx] = match &steps {
                    Some(steps) => super::gradient_over(&steps[t_idx], buf[idx], luts),
                    None => blend_pixel(lut[t_idx], buf[idx], mode),
                };
            }
        }
    }

    /// The nearest-sampled scaled image loop, with the span arithmetic `draw_image` uses.
    #[allow(clippy::too_many_arguments)]
    fn reference_scaled(
        image: &Image,
        src: Rect,
        dest: Rect,
        mode: BlendMode,
        alpha: f32,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let centre_span = |from: f32, to: f32, lo: u32, hi: u32| {
            (
                f32_to_i32_clamped((from - 0.5).ceil()).max(lo as i32),
                f32_to_i32_clamped((to - 0.5).ceil()).min(hi as i32),
            )
        };
        let (x0, x1) = centre_span(dest.x, dest.x + dest.width, clip.x0, clip.x1);
        let (y0, y1) = centre_span(dest.y, dest.y + dest.height, clip.y0, clip.y1);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let max_sx = image.width as f32 - 0.001;
        let max_sy = image.height as f32 - 0.001;
        for cy in y0..y1 {
            let v = (cy as f32 + 0.5 - dest.y) / dest.height * src.height + src.y;
            let sy = f32_to_u32_clamped(v.clamp(0.0, max_sy));
            let src_row = &image.data[sy as usize * image.width as usize * 4..];
            for cx in x0..x1 {
                let u = (cx as f32 + 0.5 - dest.x) / dest.width * src.width + src.x;
                let sx = f32_to_u32_clamped(u.clamp(0.0, max_sx));
                let ii = sx as usize * 4;
                if ii + 3 >= src_row.len() {
                    continue;
                }
                let rgba = (u32::from(pixel::scale_u8(src_row[ii + 3], alpha)) << 24)
                    | (u32::from(src_row[ii]) << 16)
                    | (u32::from(src_row[ii + 1]) << 8)
                    | u32::from(src_row[ii + 2]);
                pixel::put(&mut Band::whole(buf), bw, clip, cx, cy, rgba, mode);
            }
        }
    }

    /// Mixed opaque, translucent, transparent and empty destination pixels.
    fn background(len: u32) -> Vec<u32> {
        (0..len)
            .map(|i| {
                let h = i.wrapping_mul(2_654_435_761);
                match i % 5 {
                    0 => 0,
                    1 | 3 => 0xff00_0000 | (h & 0x00ff_ffff),
                    _ => (h & 0xff00_0000) | (h & 0x00ff_ffff),
                }
            })
            .collect()
    }

    fn stop(offset: f32, r: u8, g: u8, b: u8, a: u8) -> GradientStop {
        GradientStop {
            offset,
            color: Color::rgba(r, g, b, a),
        }
    }

    fn stop_sets() -> Vec<Vec<GradientStop>> {
        vec![
            vec![stop(0.0, 255, 255, 255, 255), stop(1.0, 255, 255, 255, 0)],
            vec![stop(0.0, 0, 0, 0, 0), stop(1.0, 0, 0, 0, 255)],
            vec![stop(0.0, 255, 0, 0, 255), stop(1.0, 0, 0, 255, 255)],
            vec![
                stop(0.0, 200, 30, 10, 90),
                stop(0.4, 10, 200, 30, 200),
                stop(1.0, 30, 40, 250, 0),
            ],
            vec![stop(0.2, 90, 90, 90, 128), stop(0.8, 20, 20, 20, 64)],
        ]
    }

    fn clips() -> Vec<ClipBox> {
        vec![
            ClipBox::surface(W, H),
            ClipBox {
                x0: 3,
                y0: 2,
                x1: 33,
                y1: 25,
            },
            ClipBox::surface(W, H).intersect(Rect::new(2.4, 1.6, 17.2, 14.9)),
        ]
    }

    fn assert_same(got: &[u32], want: &[u32], case: &str) {
        for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
            assert_eq!(
                g,
                w,
                "{case}: pixel ({}, {})",
                i % W as usize,
                i / W as usize
            );
        }
    }

    /// Within one level per channel, for the integer gradient mix.
    fn assert_close(got: &[u32], want: &[u32], case: &str) {
        for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
            let off = [0, 8, 16, 24].map(|s| ((g >> s) & 255).abs_diff((w >> s) & 255));
            assert!(
                off.iter().all(|&d| d <= 1),
                "{case}: pixel ({}, {}) {g:08x} vs {w:08x}",
                i % W as usize,
                i / W as usize
            );
        }
    }

    #[test]
    fn faded_gradient_helpers_match_struct_update() {
        let stops = vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgba(10, 20, 30, 200),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgba(250, 240, 230, 255),
            },
        ];
        let linear = LinearGradient {
            start: Point::new(1.5, 2.0),
            end: Point::new(40.0, 9.25),
            stops: stops.clone(),
        };
        let radial = RadialGradient {
            center: Point::new(12.0, 7.5),
            radius: 9.0,
            stops,
        };
        for alpha in [0.0, 0.25, 0.5, 0.99] {
            let old = LinearGradient {
                stops: super::fade_stops(&linear.stops, alpha),
                ..linear.clone()
            };
            let new = super::fade_linear(&linear, alpha);
            assert_eq!(
                format!("{new:?}"),
                format!("{old:?}"),
                "linear alpha {alpha}"
            );

            let old = RadialGradient {
                stops: super::fade_stops(&radial.stops, alpha),
                ..radial.clone()
            };
            let new = super::fade_radial(&radial, alpha);
            assert_eq!(
                format!("{new:?}"),
                format!("{old:?}"),
                "radial alpha {alpha}"
            );
        }
    }

    #[test]
    fn linear_gradients_match_the_reference() {
        let rects = [
            Rect::new(0.0, 0.0, W as f32, H as f32),
            Rect::new(1.5, 2.25, 33.5, 20.5),
            Rect::new(4.0, 1.0, 20.0, 25.0),
            Rect::new(-4.0, -3.0, 50.0, 40.0),
            Rect::new(0.5, 0.25, 0.75, 29.5),
            Rect::new(7.25, 3.5, 2.0, 1.0),
        ];
        let axes = [
            (Point::new(2.0, 0.0), Point::new(30.0, 0.0)),
            (Point::new(30.0, 7.25), Point::new(2.0, 7.25)),
            (Point::new(0.0, 0.0), Point::new(0.0, H as f32)),
            (Point::new(11.0, 28.0), Point::new(11.0, 0.0)),
            (Point::new(7.5, 1.0), Point::new(7.5, 26.5)),
            (Point::new(5.0, 3.0), Point::new(25.0, 22.0)),
        ];
        for stops in stop_sets() {
            for (start, end) in axes {
                let grad = LinearGradient {
                    start,
                    end,
                    stops: stops.clone(),
                };
                for rect in rects {
                    for clip in clips() {
                        for mode in [BlendMode::Normal, BlendMode::Multiply] {
                            let mut want = background(W * H);
                            reference_linear(&grad, rect, mode, &mut want, W, clip);
                            let mut got = background(W * H);
                            super::fill_linear(
                                &grad,
                                rect,
                                mode,
                                &mut Band::whole(&mut got),
                                W,
                                clip,
                            );
                            assert_same(
                                &got,
                                &want,
                                &format!("{mode:?} {start:?}->{end:?} {rect:?}"),
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn radial_gradients_match_the_reference() {
        let rects = [
            Rect::new(0.0, 0.0, W as f32, H as f32),
            Rect::new(3.0, 2.0, 30.0, 25.0),
        ];
        for stops in stop_sets() {
            let grad = RadialGradient {
                center: Point::new(14.5, 12.25),
                radius: 9.0,
                stops,
            };
            for rect in rects {
                for clip in clips() {
                    for mode in [BlendMode::Normal, BlendMode::Multiply] {
                        let mut want = background(W * H);
                        reference_radial(&grad, rect, mode, &mut want, W, clip);
                        let mut got = background(W * H);
                        super::fill_radial(&grad, rect, mode, &mut Band::whole(&mut got), W, clip);
                        assert_close(&got, &want, &format!("{mode:?} radial {rect:?}"));
                    }
                }
            }
        }
    }

    fn image_pattern(w: u32, h: u32, alpha: impl Fn(u32, u32) -> u8) -> Image {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = ((x * 37) ^ (y * 91)).to_le_bytes()[0];
                data.extend([v, v.wrapping_mul(3), 255 - v, alpha(x, y)]);
            }
        }
        Image::new(w, h, data)
    }

    #[test]
    fn scaled_images_match_the_reference() {
        let images = [
            image_pattern(7, 5, |_, _| 255),
            image_pattern(7, 5, |x, y| [255, 0, 128, 255, 7][((x + y) % 5) as usize]),
            image_pattern(7, 5, |_, _| 128),
        ];
        // Up, down, partly offscreen, fractional, and a sub-rectangle of the source.
        let cases = [
            (
                Rect::new(0.0, 0.0, 7.0, 5.0),
                Rect::new(-3.0, 4.5, 60.0, 40.0),
            ),
            (Rect::new(0.0, 0.0, 7.0, 5.0), Rect::new(2.0, 1.0, 3.0, 2.0)),
            (
                Rect::new(0.0, 0.0, 7.0, 5.0),
                Rect::new(1.25, 3.5, 22.5, 17.75),
            ),
            (
                Rect::new(1.0, 1.0, 5.0, 3.0),
                Rect::new(-5.0, -2.0, 50.0, 31.0),
            ),
        ];
        for image in &images {
            for (src, dest) in cases {
                for clip in clips() {
                    for mode in [BlendMode::Normal, BlendMode::Multiply] {
                        for alpha in [1.0, 0.5] {
                            let mut want = background(W * H);
                            reference_scaled(image, src, dest, mode, alpha, &mut want, W, clip);
                            let mut got = background(W * H);
                            image::draw(
                                image,
                                src,
                                dest,
                                Sampling::Nearest,
                                mode,
                                alpha,
                                &mut Band::whole(&mut got),
                                &mut ImageScratch::default(),
                                W,
                                clip,
                            );
                            assert_same(&got, &want, &format!("{mode:?} {alpha} {dest:?}"));
                        }
                    }
                }
            }
        }
    }
}
