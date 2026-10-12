//! Turning a stroked path into something the filler can fill.

use crate::types::{Path, PathCommand, Point};
use std::mem::take;

/// A run of points from one `MoveTo` to the next, with whether it closed.
pub struct SubPath {
    pub points: Vec<Point>,
    pub closed: bool,
}

/// Splits a path into runs of straight points, subdividing any curves.
pub fn flatten(path: &Path) -> Vec<SubPath> {
    let mut runs: Vec<SubPath> = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut cursor = Point::new(0.0, 0.0);

    for command in &path.commands {
        match command {
            PathCommand::MoveTo(p) => {
                push_run(&mut runs, &mut current, false);
                cursor = *p;
                current.push(cursor);
            }
            PathCommand::LineTo(p) => {
                cursor = *p;
                current.push(cursor);
            }
            PathCommand::QuadTo(c, p) => {
                let steps: u16 = 4;
                for i in 1..=steps {
                    let t = f32::from(i) / f32::from(steps);
                    current.push(quadratic(cursor, *c, *p, t));
                }
                cursor = *p;
            }
            PathCommand::CubicTo(c1, c2, p) => {
                let steps: u16 = 8;
                for i in 1..=steps {
                    let t = f32::from(i) / f32::from(steps);
                    current.push(cubic(cursor, *c1, *c2, *p, t));
                }
                cursor = *p;
            }
            PathCommand::Close => {
                if let Some(first) = current.first().copied() {
                    cursor = first;
                }
                push_run(&mut runs, &mut current, true);
            }
        }
    }
    push_run(&mut runs, &mut current, false);
    runs
}

fn push_run(runs: &mut Vec<SubPath>, current: &mut Vec<Point>, closed: bool) {
    if current.len() >= 2 {
        runs.push(SubPath {
            points: take(current),
            closed,
        });
    } else {
        current.clear();
    }
}

/// The outline of `path` stroked at `width`, as convex pieces to fill.
pub fn outline(path: &Path, width: f32) -> Vec<Path> {
    let half = (width / 2.0).max(0.05);
    let mut pieces = Vec::new();

    for run in flatten(path) {
        let points = dedup(&run.points, run.closed);
        if points.len() < 2 {
            continue;
        }
        append_pieces(&mut pieces, &points, half, run.closed);
    }
    pieces
}

fn dedup(points: &[Point], closed: bool) -> Vec<Point> {
    let mut kept: Vec<Point> = Vec::with_capacity(points.len());
    for point in points {
        match kept.last() {
            Some(last) if near(*last, *point) => {}
            _ => kept.push(*point),
        }
    }
    if closed
        && kept.len() > 2
        && let (Some(first), Some(last)) = (kept.first().copied(), kept.last().copied())
        && near(first, last)
    {
        kept.pop();
    }
    kept
}

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
}

const MAX_RUN: usize = 48;

fn append_pieces(pieces: &mut Vec<Path>, points: &[Point], half: f32, closed: bool) {
    let segments = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    if segments == 0 {
        return;
    }

    let mut run: Vec<Point> = vec![points[0]];
    for i in 0..segments {
        let next = points[(i + 1) % points.len()];
        run.push(next);

        let is_last = i + 1 == segments;
        if is_last && !closed {
            continue;
        }

        let following = following_of(points, i);
        if let Some(outward) = wedge_side(points[i], next, following, half) {
            outline_run(pieces, &run, half);
            push_wedge(pieces, points[i], next, following, half, outward);
            run = vec![next];
        } else if run.len() >= MAX_RUN {
            outline_run(pieces, &run, half);
            run = vec![next];
        }
    }
    outline_run(pieces, &run, half);
}

fn following_of(points: &[Point], i: usize) -> Point {
    points[(i + 2) % points.len()]
}

fn outline_run(pieces: &mut Vec<Path>, run: &[Point], half: f32) {
    if run.len() < 2 {
        return;
    }
    let mut left: Vec<Point> = Vec::with_capacity(run.len() * 2);
    let mut right: Vec<Point> = Vec::with_capacity(run.len() * 2);

    for pair in run.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let Some(n) = normal_of(a, b, half) else {
            continue;
        };
        left.push(Point::new(a.x + n.x, a.y + n.y));
        left.push(Point::new(b.x + n.x, b.y + n.y));
        right.push(Point::new(a.x - n.x, a.y - n.y));
        right.push(Point::new(b.x - n.x, b.y - n.y));
    }
    if left.len() < 2 {
        return;
    }

    let mut ring = left;
    ring.extend(right.into_iter().rev());
    pieces.push(polygon(&ring));
}

fn push_wedge(
    pieces: &mut Vec<Path>,
    previous: Point,
    corner: Point,
    following: Point,
    half: f32,
    outward: f32,
) {
    let (Some(n1), Some(n2)) = (
        normal_of(previous, corner, half),
        normal_of(corner, following, half),
    ) else {
        return;
    };
    pieces.push(polygon(&[
        corner,
        Point::new(corner.x + outward * n1.x, corner.y + outward * n1.y),
        Point::new(corner.x + outward * n2.x, corner.y + outward * n2.y),
    ]));
}

fn wedge_side(previous: Point, corner: Point, following: Point, half: f32) -> Option<f32> {
    let (d1x, d1y) = unit(previous, corner)?;
    let (d2x, d2y) = unit(corner, following)?;

    let cross = d1x * d2y - d1y * d2x;
    let dot = (d1x * d2x + d1y * d2y).clamp(-1.0, 1.0);

    let denominator = 1.0 + dot;
    if denominator <= f32::EPSILON {
        return Some(if cross > 0.0 { -1.0 } else { 1.0 });
    }
    let notch = half * (cross.abs() / denominator);
    if notch < 0.33 {
        return None;
    }
    Some(if cross > 0.0 { -1.0 } else { 1.0 })
}

fn unit(a: Point, b: Point) -> Option<(f32, f32)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx.hypot(dy);
    if length < 1e-6 {
        return None;
    }
    Some((dx / length, dy / length))
}

fn polygon(points: &[Point]) -> Path {
    let mut path = Path::new();
    let Some(first) = points.first() else {
        return path;
    };
    path.commands.push(PathCommand::MoveTo(*first));
    for point in points.iter().skip(1) {
        path.commands.push(PathCommand::LineTo(*point));
    }
    path.commands.push(PathCommand::Close);
    path
}

fn normal_of(a: Point, b: Point, half: f32) -> Option<Point> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx.hypot(dy);
    if length < 1e-6 {
        return None;
    }
    Some(Point::new(-dy / length * half, dx / length * half))
}

fn quadratic(p0: Point, p1: Point, p2: Point, t: f32) -> Point {
    let mt = 1.0 - t;
    Point::new(
        mt * mt * p0.x + 2.0 * mt * t * p1.x + t * t * p2.x,
        mt * mt * p0.y + 2.0 * mt * t * p1.y + t * t * p2.y,
    )
}

fn cubic(p0: Point, p1: Point, p2: Point, p3: Point, t: f32) -> Point {
    let mt = 1.0 - t;
    let a = mt * mt * mt;
    let b = 3.0 * mt * mt * t;
    let c = 3.0 * mt * t * t;
    let d = t * t * t;
    Point::new(
        a * p0.x + b * p1.x + c * p2.x + d * p3.x,
        a * p0.y + b * p1.y + c * p2.y + d * p3.y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(points: &[(f32, f32)]) -> Path {
        let mut path = Path::new();
        let mut first = true;
        for (x, y) in points {
            let p = Point::new(*x, *y);
            if first {
                path.commands.push(PathCommand::MoveTo(p));
                first = false;
            } else {
                path.commands.push(PathCommand::LineTo(p));
            }
        }
        path
    }

    fn points_of(pieces: &[Path]) -> Vec<Point> {
        pieces
            .iter()
            .flat_map(|p| p.commands.iter())
            .filter_map(|c| match c {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => Some(*p),
                _ => None,
            })
            .collect()
    }

    fn bounds(pieces: &[Path]) -> (f32, f32, f32, f32) {
        let points = points_of(pieces);
        let xs: Vec<f32> = points.iter().map(|p| p.x).collect();
        let ys: Vec<f32> = points.iter().map(|p| p.y).collect();
        (
            xs.iter().copied().fold(f32::INFINITY, f32::min),
            xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            ys.iter().copied().fold(f32::INFINITY, f32::min),
            ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        )
    }

    #[test]
    fn a_straight_line_strokes_to_one_quad() {
        let pieces = outline(&line(&[(0.0, 10.0), (20.0, 10.0)]), 4.0);

        assert_eq!(pieces.len(), 1, "one segment, no joins");
        assert_eq!(points_of(&pieces).len(), 4, "a quad has four corners");
    }

    #[test]
    fn a_reversal_is_always_treated_as_sharp() {
        let pieces = outline(&line(&[(0.0, 0.0), (10.0, 0.0), (0.0, 0.0)]), 2.0);
        assert!(pieces.len() > 1, "the run must be cut at a reversal");
    }

    #[test]
    fn the_outline_is_as_wide_as_the_stroke() {
        let pieces = outline(&line(&[(0.0, 0.0), (0.0, 20.0)]), 6.0);
        let (min_x, max_x, _, _) = bounds(&pieces);

        assert!(
            (max_x - min_x - 6.0).abs() < 1e-4,
            "width was {}",
            max_x - min_x
        );
    }

    #[test]
    fn the_outline_covers_the_length_of_the_line() {
        let pieces = outline(&line(&[(0.0, 10.0), (20.0, 10.0)]), 2.0);
        let (min_x, max_x, _, _) = bounds(&pieces);

        assert!((min_x - 0.0).abs() < 1e-4 && (max_x - 20.0).abs() < 1e-4);
    }

    #[test]
    fn a_sharp_bend_cuts_the_run_and_adds_a_wedge() {
        let pieces = outline(&line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]), 2.0);
        assert_eq!(pieces.len(), 3, "a run each side, and the wedge between");
    }

    #[test]
    fn a_gentle_turn_does_not_cut_the_run() {
        let points: Vec<(f32, f32)> = (0..20u8)
            .map(|i| {
                let x = f32::from(i) * 4.0;
                (x, 50.0 + x * 0.02)
            })
            .collect();
        let pieces = outline(&line(&points), 1.5);

        assert_eq!(
            pieces.len(),
            1,
            "nineteen near-straight segments, one piece"
        );
    }

    #[test]
    fn a_very_long_run_is_broken_into_pieces() {
        let points: Vec<(f32, f32)> = (0..200u8).map(|i| (f32::from(i) * 2.0, 50.0)).collect();
        let pieces = outline(&line(&points), 1.5);

        assert!(pieces.len() > 1, "got {}", pieces.len());
        assert!(
            pieces.len() < 20,
            "but not one per segment: {}",
            pieces.len()
        );
    }

    #[test]
    fn repeated_points_are_dropped() {
        let pieces = outline(&line(&[(0.0, 0.0), (0.0, 0.0), (10.0, 0.0)]), 2.0);
        assert_eq!(pieces.len(), 1, "still one segment");
    }

    #[test]
    fn a_single_point_strokes_to_nothing() {
        assert!(outline(&line(&[(5.0, 5.0)]), 2.0).is_empty());
    }

    #[test]
    fn an_empty_path_strokes_to_nothing() {
        assert!(outline(&Path::new(), 2.0).is_empty());
    }

    #[test]
    fn a_zero_width_stroke_still_has_some_body() {
        let pieces = outline(&line(&[(0.0, 0.0), (10.0, 0.0)]), 0.0);
        assert!(!pieces.is_empty());
        let (_, _, min_y, max_y) = bounds(&pieces);
        assert!(max_y > min_y, "it should still have a thickness");
    }

    #[test]
    fn a_closed_run_strokes_all_the_way_round() {
        let mut path = line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        path.commands.push(PathCommand::Close);

        let pieces = outline(&path, 2.0);
        assert_eq!(pieces.len(), 4 + 4, "four sides and four corners");
    }

    #[test]
    fn separate_subpaths_stroke_separately() {
        let mut path = line(&[(0.0, 0.0), (10.0, 0.0)]);
        path.commands
            .push(PathCommand::MoveTo(Point::new(0.0, 20.0)));
        path.commands
            .push(PathCommand::LineTo(Point::new(10.0, 20.0)));

        assert_eq!(outline(&path, 2.0).len(), 2);
    }

    #[test]
    fn curves_are_subdivided_before_stroking() {
        let mut path = Path::new();
        path.commands
            .push(PathCommand::MoveTo(Point::new(0.0, 0.0)));
        path.commands.push(PathCommand::CubicTo(
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
        ));

        let runs = flatten(&path);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].points.len() > 2, "the curve should be flattened");
    }
}
