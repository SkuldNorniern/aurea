//! Turning a stroked path into something the filler can fill.
//!
//! The scanline filler only fills. A stroke is therefore built as an outline
//! polygon around the path and filled like any other shape, which also means a
//! stroke gets the same coverage antialiasing as a fill instead of a separate,
//! harder-edged code path.
//!
//! The outline comes back as separate convex pieces, one quad per segment plus
//! a bevel wedge at each turn, and each piece is filled on its own. A single
//! self-overlapping outline would not work: the filler pairs crossings with the
//! odd-even rule, so where two parts of one shape overlap the crossings cancel
//! and a hole opens up in the middle of the join.

use crate::types::{Path, PathCommand, Point};
use std::mem::take;

/// A run of points from one `MoveTo` to the next, with whether it closed.
pub struct SubPath {
    pub points: Vec<Point>,
    pub closed: bool,
}

/// Splits a path into runs of straight points, subdividing any curves.
///
/// The step counts match the filler's own tessellation, so a shape looks the
/// same whether it is filled or stroked.
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
///
/// Each piece is filled separately, so nothing overlaps within one fill and no
/// join can cancel itself out. A run that is a single point, or a width of
/// zero, contributes nothing rather than a degenerate shape.
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

/// Drops repeated points, which have no direction and would produce a zero
/// normal.
fn dedup(points: &[Point], closed: bool) -> Vec<Point> {
    let mut kept: Vec<Point> = Vec::with_capacity(points.len());
    for point in points {
        match kept.last() {
            Some(last) if near(*last, *point) => {}
            _ => kept.push(*point),
        }
    }
    // A closed run repeats its first point at the end; the loop is implied.
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

/// One quad per segment, plus a wedge to close the notch at each turn.
fn append_pieces(pieces: &mut Vec<Path>, points: &[Point], half: f32, closed: bool) {
    let segments = if closed {
        points.len()
    } else {
        points.len() - 1
    };

    let mut normals: Vec<Option<Point>> = Vec::with_capacity(segments);
    for i in 0..segments {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        normals.push(normal_of(a, b, half));

        if let Some(n) = normals[i] {
            pieces.push(polygon(&[
                Point::new(a.x + n.x, a.y + n.y),
                Point::new(b.x + n.x, b.y + n.y),
                Point::new(b.x - n.x, b.y - n.y),
                Point::new(a.x - n.x, a.y - n.y),
            ]));
        }
    }

    // At each turn the two segments leave a gap on the outside. One of these
    // two wedges fills it; the other lands inside the stroke and is harmless.
    let turns = if closed { segments } else { segments - 1 };
    for i in 0..turns {
        let next = (i + 1) % segments;
        let (Some(n1), Some(n2)) = (normals[i], normals[next]) else {
            continue;
        };
        let v = points[(i + 1) % points.len()];
        pieces.push(polygon(&[
            v,
            Point::new(v.x + n1.x, v.y + n1.y),
            Point::new(v.x + n2.x, v.y + n2.y),
        ]));
        pieces.push(polygon(&[
            v,
            Point::new(v.x - n1.x, v.y - n1.y),
            Point::new(v.x - n2.x, v.y - n2.y),
        ]));
    }
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

/// A vector `half` long, at right angles to `a` -> `b`.
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

    /// Every point across all pieces.
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

    /// Each piece is convex and filled on its own, so a bend produces the two
    /// segment quads plus the wedges that close the notch.
    #[test]
    fn a_bend_adds_a_wedge_at_the_turn() {
        let pieces = outline(&line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]), 2.0);
        assert_eq!(pieces.len(), 4, "two quads and two wedges");
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
        // Otherwise a hairline would vanish instead of drawing thin.
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
        // Four sides, and a join at every corner including the closing one.
        assert_eq!(pieces.len(), 4 + 4 * 2);
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
