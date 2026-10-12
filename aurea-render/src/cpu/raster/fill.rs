//! Path filling with antialiasing on both axes.

use std::cmp::Ordering;

use crate::cpu::blend::blend_pixel;
use crate::cpu::clip::ClipBox;
use crate::cpu::path::{Edge, tessellate_path_into};
use crate::geometry::FillRule;
use crate::numeric::{f32_to_u32_clamped, f32_to_usize_clamped};
use crate::types::{BlendMode, Color, Path};

const SUBSAMPLES: u32 = 4;

/// Buffers a fill reuses from one path to the next.
#[derive(Default)]
pub struct PathScratch {
    edges: Vec<Edge>,
    active: Vec<usize>,
    crossings: Vec<(f32, i32)>,
    coverage: Vec<f32>,
}

/// Fills `path` in `color` into `buf` (`bw` pixels wide), inside `clip`.
pub fn fill_path(
    path: &Path,
    color: Color,
    mode: BlendMode,
    buf: &mut [u32],
    bw: u32,
    clip: ClipBox,
    s: &mut PathScratch,
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

    s.coverage.clear();
    s.coverage.resize(bw as usize, 0.0);
    s.active.clear();
    let (left, right) = (clip.left().max(0.0), clip.right().min(bw as f32));
    let weight = 1.0 / SUBSAMPLES as f32;
    let mut enter = 0;
    for y in y_start..y_end {
        let mut touched = (usize::MAX, 0usize);
        for sub in 0..SUBSAMPLES {
            let sy = y as f32 + (sub as f32 + 0.5) * weight;
            while enter < s.edges.len() && s.edges[enter].y_min <= sy {
                s.active.push(enter);
                enter += 1;
            }
            let edges = &s.edges;
            s.active.retain(|&i| edges[i].y_max > sy);
            s.crossings.clear();
            s.crossings.extend(
                s.active
                    .iter()
                    .filter(|&&i| edges[i].y_min <= sy)
                    .map(|&i| (edges[i].x_at_y(sy), edges[i].dir)),
            );
            s.crossings
                .sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
            add_spans(
                &s.crossings,
                path.fill_rule,
                (left, right),
                weight,
                &mut s.coverage,
                &mut touched,
            );
        }
        if touched.0 <= touched.1 {
            let row = y as usize * bw as usize;
            composite_row(
                &mut s.coverage[touched.0..=touched.1],
                &mut buf[row + touched.0..],
                color,
                mode,
            );
        }
    }
}

fn add_spans(
    crossings: &[(f32, i32)],
    rule: FillRule,
    (left, right): (f32, f32),
    weight: f32,
    coverage: &mut [f32],
    touched: &mut (usize, usize),
) {
    let mut winding = 0;
    let mut start = 0.0;
    for &(x, dir) in crossings {
        let was_inside = inside(winding, rule);
        winding += dir;
        match (was_inside, inside(winding, rule)) {
            (false, true) => start = x,
            (true, false) => add_span(start.max(left), x.min(right), weight, coverage, touched),
            _ => {}
        }
    }
}

fn inside(winding: i32, rule: FillRule) -> bool {
    match rule {
        FillRule::NonZero => winding != 0,
        FillRule::EvenOdd => winding % 2 != 0,
    }
}

fn add_span(a: f32, b: f32, weight: f32, coverage: &mut [f32], touched: &mut (usize, usize)) {
    if b <= a {
        return;
    }
    let (first, last) = (
        f32_to_usize_clamped(a.floor()),
        f32_to_usize_clamped(b.ceil()) - 1,
    );
    let last = last.min(coverage.len().saturating_sub(1));
    for (i, c) in coverage.iter_mut().enumerate().take(last + 1).skip(first) {
        let px = i as f32;
        *c += weight * (b.min(px + 1.0) - a.max(px));
    }
    *touched = (touched.0.min(first), touched.1.max(last));
}

fn composite_row(coverage: &mut [f32], row: &mut [u32], color: Color, mode: BlendMode) {
    let opaque = (u32::from(color.a) << 24)
        | (u32::from(color.r) << 16)
        | (u32::from(color.g) << 8)
        | u32::from(color.b);
    let solid = mode == BlendMode::Normal && color.a == 255;
    for (c, px) in coverage.iter_mut().zip(row.iter_mut()) {
        let cov = c.min(1.0);
        *c = 0.0;
        if cov <= 0.0 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PathCommand, Point};

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
        let mut buf = vec![0u32; 16 * 16];
        let mut s = PathScratch::default();
        fill_path(
            path,
            Color::rgb(255, 255, 255),
            BlendMode::Normal,
            &mut buf,
            16,
            ClipBox::surface(16, 16),
            &mut s,
        );
        buf
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
}
