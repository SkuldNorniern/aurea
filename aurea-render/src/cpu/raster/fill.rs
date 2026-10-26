//! Path filling with exact area coverage.

use std::cmp::Ordering;

use crate::cpu::blend::{ConstSrc, blend_pixel};
use crate::cpu::clip::ClipBox;
use crate::cpu::path::{Edge, tessellate_path_into};
use crate::cpu::raster::band::Band;
use crate::geometry::FillRule;
use crate::numeric::{f32_to_u32_clamped, f32_to_usize_clamped};
use crate::types::{BlendMode, Color, Path};

const NO_COVERAGE: f32 = 0.5 / 255.0;

/// Buffers a fill reuses from one path to the next.
#[derive(Default)]
pub struct PathScratch {
    edges: Vec<Edge>,
    active: Vec<usize>,
    coverage: Vec<f32>,
}

/// Fills `path` in `color` into `buf` (`bw` pixels wide), inside `clip`.
pub fn fill_path(
    path: &Path,
    color: Color,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
    s: &mut PathScratch,
) {
    fill_path_with(path, buf, bw, clip, s, |area, row, rule| {
        composite_row(area, row, rule, color, mode);
    });
}

/// [`fill_path`] with the per-row compositing supplied by `composite`.
fn fill_path_with(
    path: &Path,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
    s: &mut PathScratch,
    mut composite: impl FnMut(&[f32], &mut [u32], FillRule),
) {
    tessellate_path_into(path, &mut s.edges);
    if s.edges.is_empty() || clip.is_empty() {
        return;
    }
    s.edges
        .sort_unstable_by(|a, b| a.y_min.partial_cmp(&b.y_min).unwrap_or(Ordering::Equal));
    let y_min = s.edges[0].y_min;
    let y_max = s.edges.iter().map(|e| e.y_max).fold(f32::MIN, f32::max);
    let y_start = f32_to_u32_clamped(y_min.max(clip.top()).floor()).max(clip.y0);
    let y_end = f32_to_u32_clamped(y_max.min(clip.bottom()).ceil()).min(clip.y1);

    let right_px = clip.x1.min(bw) as usize;
    s.coverage.clear();
    s.coverage.resize(right_px + 2, 0.0);
    s.active.clear();
    let (left, right) = (clip.left().max(0.0), right_px as f32);
    let mut enter = 0;
    for y in y_start..y_end {
        let (top, bottom) = (y as f32, y as f32 + 1.0);
        while enter < s.edges.len() && s.edges[enter].y_min < bottom {
            s.active.push(enter);
            enter += 1;
        }
        let edges = &s.edges;
        s.active.retain(|&i| edges[i].y_max > top);
        let mut touched = (usize::MAX, 0usize);
        for &i in &s.active {
            let e = &edges[i];
            let (y0, y1) = (e.y_min.max(top), e.y_max.min(bottom));
            if y1 <= y0 {
                continue;
            }
            let (x0, x1) = (e.x_at_y(y0), e.x_at_y(y1));
            let area = (y1 - y0) * e.dir as f32;
            add_edge(
                x0.clamp(left, right),
                x1.clamp(left, right),
                area,
                &mut s.coverage,
                &mut touched,
            );
        }
        if touched.0 > touched.1 {
            continue;
        }
        let last = touched.1.min(right_px.saturating_sub(1));
        let row = y as usize * bw as usize;
        if touched.0 <= last {
            composite(
                &s.coverage[touched.0..=last],
                &mut buf[row + touched.0..],
                path.fill_rule,
            );
        }
        s.coverage[touched.0..=touched.1].fill(0.0);
    }
}

fn add_edge(x0: f32, x1: f32, area: f32, coverage: &mut [f32], touched: &mut (usize, usize)) {
    let (lo, hi) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
    let c0 = f32_to_usize_clamped(lo.floor());
    let c1 = f32_to_usize_clamped(hi.ceil()).max(c0 + 1);
    if c1 - c0 == 1 {
        let mid = (lo + hi) * 0.5 - c0 as f32;
        coverage[c0] += area * (1.0 - mid);
        coverage[c0 + 1] += area * mid;
    } else {
        let per_x = area / (hi - lo);
        for c in c0..c1 {
            let (a, b) = ((c as f32).max(lo), (c as f32 + 1.0).min(hi));
            let share = (b - a) * per_x;
            let mid = (a + b) * 0.5 - c as f32;
            coverage[c] += share * (1.0 - mid);
            coverage[c + 1] += share * mid;
        }
    }
    *touched = (touched.0.min(c0), touched.1.max(c1));
}

fn rule_coverage(sum: f32, rule: FillRule) -> f32 {
    match rule {
        FillRule::NonZero => sum.abs().min(1.0),
        FillRule::EvenOdd => 1.0 - (sum.abs().rem_euclid(2.0) - 1.0).abs(),
    }
}

/// Runs shorter than this are shaded one pixel at a time.
const SHORT_RUN: usize = 8;

/// One colour and blend mode, prepared for compositing rows.
struct Paint {
    source: ConstSrc,
    opaque: u32,
    alpha: f32,
    solid: bool,
    mode: BlendMode,
}

impl Paint {
    fn new(color: Color, mode: BlendMode) -> Self {
        let opaque = (u32::from(color.a) << 24)
            | (u32::from(color.r) << 16)
            | (u32::from(color.g) << 8)
            | u32::from(color.b);
        Self {
            source: ConstSrc::new(opaque),
            opaque,
            alpha: f32::from(color.a),
            solid: mode == BlendMode::Normal && color.a == 255,
            mode,
        }
    }

    fn alpha_at(&self, cov: f32) -> u32 {
        f32_to_u32_clamped((self.alpha * cov).round())
    }

    /// One pixel at coverage `cov` (at least [`NO_COVERAGE`]).
    fn pixel(&self, dst: u32, cov: f32) -> u32 {
        if self.solid && cov >= 0.999 {
            return self.opaque;
        }
        let a = self.alpha_at(cov);
        if self.mode == BlendMode::Normal {
            self.source.over_at_alpha(dst, a)
        } else {
            blend_pixel((a << 24) | (self.opaque & 0x00FF_FFFF), dst, self.mode)
        }
    }

    /// A run of pixels that all share coverage `cov` (at least [`NO_COVERAGE`]).
    fn run(&self, px: &mut [u32], cov: f32) {
        if self.solid && cov >= 0.999 {
            px.fill(self.opaque);
            return;
        }
        let a = self.alpha_at(cov);
        if self.mode == BlendMode::Normal {
            self.source.over_span_at_alpha(px, a);
        } else {
            let src = (a << 24) | (self.opaque & 0x00FF_FFFF);
            for p in px {
                *p = blend_pixel(src, *p, self.mode);
            }
        }
    }
}

fn composite_row(area: &[f32], row: &mut [u32], rule: FillRule, color: Color, mode: BlendMode) {
    let paint = Paint::new(color, mode);
    let n = area.len().min(row.len());
    let (area, row) = (&area[..n], &mut row[..n]);
    let mut sum = 0.0;
    let mut i = 0;
    while i < n {
        sum += area[i];
        // A zero adds nothing to `sum`, so the run `i..end` shares one coverage value.
        let end = i + 1 + area[i + 1..].iter().take_while(|&&c| c == 0.0).count();
        let cov = rule_coverage(sum, rule);
        if cov >= NO_COVERAGE {
            if end - i < SHORT_RUN {
                for px in &mut row[i..end] {
                    *px = paint.pixel(*px, cov);
                }
            } else {
                paint.run(&mut row[i..end], cov);
            }
        }
        i = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PathCommand, Point};
    use std::f32::consts::{FRAC_PI_2, TAU};
    use std::time::Instant;

    fn square(path: &mut Path, x: f32, y: f32, size: f32, clockwise: bool) {
        let mut corners = [
            Point::new(x, y),
            Point::new(x + size, y),
            Point::new(x + size, y + size),
            Point::new(x, y + size),
        ];
        if !clockwise {
            corners.reverse();
        }
        path.commands.push(PathCommand::MoveTo(corners[0]));
        for &p in &corners[1..] {
            path.commands.push(PathCommand::LineTo(p));
        }
        path.commands.push(PathCommand::Close);
    }

    fn fill(path: &Path) -> Vec<u32> {
        fill_sized(path, 16, 16)
    }

    fn fill_sized(path: &Path, w: u32, h: u32) -> Vec<u32> {
        let mut buf = vec![0u32; (w * h) as usize];
        let mut s = PathScratch::default();
        fill_path(
            path,
            Color::rgb(255, 255, 255),
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            w,
            ClipBox::surface(w, h),
            &mut s,
        );
        buf
    }

    fn polygon(points: &[(f32, f32)]) -> Path {
        let mut path = Path::new();
        path.commands
            .push(PathCommand::MoveTo(Point::new(points[0].0, points[0].1)));
        for &(x, y) in &points[1..] {
            path.commands.push(PathCommand::LineTo(Point::new(x, y)));
        }
        path.commands.push(PathCommand::Close);
        path
    }

    #[test]
    fn fill_rules_differ_on_overlap() {
        let mut path = Path::new();
        square(&mut path, 2.0, 2.0, 8.0, true);
        square(&mut path, 6.0, 6.0, 8.0, true);
        assert_eq!(fill(&path)[8 * 16 + 8] >> 24, 255);
        path.fill_rule = FillRule::EvenOdd;
        assert_eq!(fill(&path)[8 * 16 + 8] >> 24, 0);
    }

    #[test]
    fn reversed_inner_contour_is_a_hole() {
        let mut path = Path::new();
        square(&mut path, 1.0, 1.0, 14.0, true);
        square(&mut path, 5.0, 5.0, 6.0, false);
        let buf = fill(&path);
        assert_eq!(buf[2 * 16 + 2] >> 24, 255);
        assert_eq!(buf[8 * 16 + 8] >> 24, 0);
    }

    #[test]
    fn edges_are_antialiased_on_both_axes() {
        let mut path = Path::new();
        square(&mut path, 4.5, 4.5, 6.0, true);
        let buf = fill(&path);
        let top = buf[4 * 16 + 7] >> 24;
        let side = buf[7 * 16 + 4] >> 24;
        assert!((120..=135).contains(&top), "top edge {top}");
        assert!((120..=135).contains(&side), "left edge {side}");
        assert_eq!(buf[7 * 16 + 7] >> 24, 255);
    }

    #[test]
    fn shallow_edges_change_every_pixel() {
        let path = polygon(&[(0.0, 4.0), (64.0, 8.0), (64.0, 16.0), (0.0, 16.0)]);
        let buf = fill_sized(&path, 64, 16);
        for y in 4..8 {
            let row: Vec<u32> = (0..64).map(|x| buf[y * 64 + x] >> 24).collect();
            let partial: Vec<u32> = row.iter().copied().filter(|&a| a > 0 && a < 255).collect();
            assert!(partial.len() >= 14, "row {y}: {row:?}");
            for pair in partial.windows(2) {
                assert_ne!(pair[0], pair[1], "row {y} plateaus: {row:?}");
            }
        }
    }

    #[test]
    fn coverage_sums_to_the_area() {
        let path = polygon(&[(1.3, 1.7), (14.2, 3.1), (5.6, 13.4)]);
        let buf = fill(&path);
        let sum: f32 = buf.iter().map(|p| (p >> 24) as f32 / 255.0).sum();
        let area = ((14.2 - 1.3) * (13.4 - 1.7) - (5.6 - 1.3) * (3.1 - 1.7)) / 2.0f32;
        assert!((sum - area).abs() < 0.3, "{sum} vs {area}");
    }

    #[test]
    fn clipping_keeps_coverage_from_outside() {
        let path = polygon(&[(-20.0, 2.0), (12.5, 2.0), (12.5, 6.0), (-20.0, 6.0)]);
        let mut buf = vec![0u32; 16 * 16];
        let clip = ClipBox {
            x0: 4,
            y0: 0,
            x1: 10,
            y1: 16,
        };
        fill_path(
            &path,
            Color::rgb(255, 255, 255),
            BlendMode::Normal,
            &mut Band::whole(&mut buf),
            16,
            clip,
            &mut PathScratch::default(),
        );
        assert_eq!(buf[3 * 16 + 3] >> 24, 0);
        assert_eq!(buf[3 * 16 + 4] >> 24, 255);
        assert_eq!(buf[3 * 16 + 9] >> 24, 255);
        assert_eq!(buf[3 * 16 + 10] >> 24, 0);
    }

    fn reference_composite(
        color: Color,
        mode: BlendMode,
    ) -> impl FnMut(&[f32], &mut [u32], FillRule) {
        let opaque = u32::from_be_bytes([color.a, color.r, color.g, color.b]);
        let solid = mode == BlendMode::Normal && color.a == 255;
        move |area, row, rule| {
            let mut sum = 0.0;
            for (c, px) in area.iter().zip(row.iter_mut()) {
                sum += c;
                let cov = rule_coverage(sum, rule);
                if cov < NO_COVERAGE {
                    continue;
                }
                if solid && cov >= 0.999 {
                    *px = opaque;
                    continue;
                }
                let a = f32_to_u32_clamped((f32::from(color.a) * cov).round());
                *px = blend_pixel((a << 24) | (opaque & 0x00FF_FFFF), *px, mode);
            }
        }
    }

    fn star_points(cx: f32, cy: f32, points: usize, outer: f32, inner: f32) -> Vec<(f32, f32)> {
        (0..points * 2)
            .map(|i| {
                let t = i as f32 / (points * 2) as f32 * TAU - FRAC_PI_2;
                let r = if i % 2 == 0 { outer } else { inner };
                (cx + r * t.cos(), cy + r * t.sin())
            })
            .collect()
    }

    fn circle_points(cx: f32, cy: f32, r: f32, n: usize, clockwise: bool) -> Vec<(f32, f32)> {
        let mut pts: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                let t = i as f32 / n as f32 * TAU;
                (cx + r * t.cos(), cy + r * t.sin())
            })
            .collect();
        if !clockwise {
            pts.reverse();
        }
        pts
    }

    #[test]
    fn composite_matches_per_pixel_blend() {
        let colors = [
            Color::rgba(200, 80, 40, 160),
            Color::rgba(20, 140, 230, 255),
            Color::rgba(255, 255, 0, 7),
        ];
        let mut small_star = polygon(&star_points(24.0, 20.0, 5, 18.0, 8.0));
        small_star.fill_rule = FillRule::EvenOdd;
        let small = [
            polygon(&[(3.3, 4.1), (41.7, 9.6), (17.2, 36.4)]),
            small_star,
            polygon(&circle_points(24.3, 20.1, 15.2, 64, true)),
        ];

        // Large shapes give long zero-area interiors, which the run fast path skips over.
        let mut big_star = polygon(&star_points(240.0, 180.0, 13, 170.0, 60.0));
        big_star.fill_rule = FillRule::EvenOdd;
        let mut ring = polygon(&circle_points(240.0, 180.0, 165.0, 128, true));
        ring.commands
            .extend(polygon(&circle_points(240.0, 180.0, 90.0, 96, false)).commands);
        let big = [big_star, ring];

        let cases = [(48u32, 40u32, small.as_slice()), (480, 360, big.as_slice())];
        for (w, h, paths) in cases {
            let backdrop: Vec<u32> = (0..w * h)
                .map(|i| {
                    let v = i.wrapping_mul(2_654_435_761);
                    ((v >> 24) << 24) | (v & 0x00ff_ffff)
                })
                .collect();
            for path in paths {
                for color in colors {
                    for mode in [BlendMode::Normal, BlendMode::Multiply] {
                        let mut actual = backdrop.clone();
                        fill_path(
                            path,
                            color,
                            mode,
                            &mut Band::whole(&mut actual),
                            w,
                            ClipBox::surface(w, h),
                            &mut PathScratch::default(),
                        );
                        let mut expected = backdrop.clone();
                        fill_path_with(
                            path,
                            &mut Band::whole(&mut expected),
                            w,
                            ClipBox::surface(w, h),
                            &mut PathScratch::default(),
                            reference_composite(color, mode),
                        );
                        assert_ne!(
                            actual, backdrop,
                            "{w}x{h} {color:?} {mode:?} painted nothing"
                        );
                        assert_eq!(actual, expected, "{w}x{h} {color:?} {mode:?}");
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "manual timing: cargo test --release -- --include-ignored --nocapture"]
    fn timing_large_translucent_fill() {
        const W: u32 = 2000;
        const H: u32 = 1500;
        const FILLS: u32 = 20;
        let mut points = Vec::new();
        for i in 0..720 {
            let t = i as f32 / 720.0 * TAU;
            let r = if i % 2 == 0 { 700.0 } else { 520.0 };
            points.push((1000.0 + r * t.cos(), 750.0 + r * t.sin()));
        }
        let path = polygon(&points);
        let color = Color::rgba(200, 80, 40, 160);
        let mut buf: Vec<u32> = (0..W * H)
            .map(|i| 0xff00_0000 | (i * 2654435761) & 0xffffff)
            .collect();
        let mut s = PathScratch::default();
        let start = Instant::now();
        for _ in 0..FILLS {
            fill_path(
                &path,
                color,
                BlendMode::Normal,
                &mut Band::whole(&mut buf),
                W,
                ClipBox::surface(W, H),
                &mut s,
            );
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(FILLS);
        eprintln!("fill {W}x{H}: {ms:.3} ms/fill");
    }

    #[test]
    #[ignore = "manual timing: cargo test --release -p aurea-render --lib timing_large_polygon -- --include-ignored --nocapture"]
    fn timing_large_polygon_fill() {
        const W: u32 = 1500;
        const H: u32 = 1000;
        const FILLS: u32 = 20;
        let mut points = Vec::new();
        for i in 0..720 {
            let t = i as f32 / 720.0 * TAU;
            let r = if i % 2 == 0 { 480.0 } else { 360.0 };
            points.push((750.0 + r * t.cos(), 500.0 + r * t.sin()));
        }
        let path = polygon(&points);
        let backdrop: Vec<u32> = (0..W * H)
            .map(|i| 0xff00_0000 | (i.wrapping_mul(2_654_435_761) & 0x00ff_ffff))
            .collect();
        for (label, color) in [
            ("opaque", Color::rgb(20, 140, 230)),
            ("translucent", Color::rgba(200, 80, 40, 160)),
        ] {
            let mut buf = backdrop.clone();
            let mut s = PathScratch::default();
            let mut best = f64::MAX;
            for _ in 0..3 {
                let start = Instant::now();
                for _ in 0..FILLS {
                    fill_path(
                        &path,
                        color,
                        BlendMode::Normal,
                        &mut Band::whole(&mut buf),
                        W,
                        ClipBox::surface(W, H),
                        &mut s,
                    );
                }
                best = best.min(start.elapsed().as_secs_f64() * 1000.0 / f64::from(FILLS));
            }
            eprintln!("{label} fill {W}x{H}: {best:.3} ms/fill (best of 3)");
        }
    }
}
