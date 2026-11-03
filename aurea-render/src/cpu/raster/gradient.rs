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
    use super::{fill_linear, fill_radial};

    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;

    use crate::types::{
        BlendMode, Color, GradientStop, LinearGradient, Point, RadialGradient, Rect,
    };

    /// Draws `fill` on a dark frame of `side` by `side`.
    fn painted(side: u32, fill: impl FnOnce(&mut Band<'_>, ClipBox)) -> Vec<u32> {
        let mut px = vec![0xFF10_1010_u32; (side * side) as usize];
        fill(&mut Band::whole(&mut px), ClipBox::surface(side, side));
        px
    }

    fn ramp(alpha: u8) -> Vec<GradientStop> {
        vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgba(250, 10, 0, alpha),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgba(0, 10, 250, alpha),
            },
        ]
    }

    #[test]
    fn a_ramp_along_either_axis_is_the_same_ramp() {
        const SIDE: u32 = 48;
        let whole = Rect::new(0.0, 0.0, SIDE as f32, SIDE as f32);
        for alpha in [255, 128] {
            let along = |end: Point| {
                let grad = LinearGradient {
                    start: Point::new(0.0, 0.0),
                    end,
                    stops: ramp(alpha),
                };
                painted(SIDE, |buf, clip| {
                    fill_linear(&grad, whole, BlendMode::Normal, buf, SIDE, clip);
                })
            };
            let across = along(Point::new(SIDE as f32, 0.0));
            let down = along(Point::new(0.0, SIDE as f32));
            let at = |x: u32, y: u32| (y * SIDE + x) as usize;
            for y in 0..SIDE {
                for x in 0..SIDE {
                    assert_eq!(across[at(x, y)], down[at(y, x)], "{x}, {y}, alpha {alpha}");
                }
            }
            let (left, right) = (across[at(0, 0)], across[at(SIDE - 1, 0)]);
            let red = |px: u32| (px >> 16) & 0xFF;
            let blue = |px: u32| px & 0xFF;
            assert!(red(left) > red(right), "{left:08x} {right:08x}");
            assert!(blue(left) < blue(right), "{left:08x} {right:08x}");
        }
    }

    #[test]
    fn an_angled_ramp_is_level_across_its_axis() {
        const SIDE: u32 = 48;
        let grad = LinearGradient {
            start: Point::new(0.0, 0.0),
            end: Point::new(SIDE as f32, SIDE as f32),
            stops: ramp(128),
        };
        let px = painted(SIDE, |buf, clip| {
            let whole = Rect::new(0.0, 0.0, SIDE as f32, SIDE as f32);
            fill_linear(&grad, whole, BlendMode::Normal, buf, SIDE, clip);
        });
        for y in 0..SIDE {
            for x in 0..SIDE {
                let (a, b) = (px[(y * SIDE + x) as usize], px[(x * SIDE + y) as usize]);
                assert_eq!(a, b, "{x}, {y}");
            }
        }
    }

    #[test]
    fn a_radial_runs_from_its_centre_to_its_edge() {
        const SIDE: u32 = 65;
        let grad = RadialGradient {
            center: Point::new(32.5, 32.5),
            radius: 20.0,
            stops: ramp(255),
        };
        let px = painted(SIDE, |buf, clip| {
            let whole = Rect::new(0.0, 0.0, SIDE as f32, SIDE as f32);
            fill_radial(&grad, whole, BlendMode::Normal, buf, SIDE, clip);
        });
        let at = |x: u32, y: u32| px[(y * SIDE + x) as usize];
        assert!((at(32, 32) >> 16) & 0xFF > 200, "{:08x}", at(32, 32));
        assert_eq!(at(0, 0), 0xFF00_0AFA, "past the edge is the last stop");
        assert_eq!(at(32, 12), at(12, 32), "the same all round");
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
}
