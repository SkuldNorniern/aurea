//! Turning a stroked path into something the filler can fill.

use super::path::{cubic_steps, quad_steps};
use crate::types::{Dash, FillRule, LineCap, LineJoin, Paint, Path, PathCommand, Point};
use std::f32::consts::PI;
use std::mem::take;

/// How a path is stroked: half its width, its ends, corners and dashes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    pub half: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f32,
    pub dash: Option<Dash>,
}

impl StrokeStyle {
    pub fn of(paint: &Paint) -> Self {
        Self {
            half: (paint.stroke_width / 2.0).max(0.05),
            cap: paint.line_cap,
            join: paint.line_join,
            miter_limit: paint.miter_limit.max(1.0),
            dash: paint.dash,
        }
    }

    #[cfg(test)]
    fn solid(width: f32) -> Self {
        Self::of(&Paint::new().stroke_width(width))
    }
}

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
                let steps = quad_steps(cursor, *c, *p);
                for i in 1..=steps {
                    let t = f32::from(i) / f32::from(steps);
                    current.push(quadratic(cursor, *c, *p, t));
                }
                cursor = *p;
            }
            PathCommand::CubicTo(c1, c2, p) => {
                let steps = cubic_steps(cursor, *c1, *c2, *p);
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

/// The outline of `path` stroked in `style`, as convex pieces to fill.
pub fn outline(path: &Path, style: &StrokeStyle) -> Vec<Path> {
    let mut pieces = Vec::new();
    for run in flatten(path) {
        let points = dedup(&run.points, run.closed);
        if points.len() < 2 {
            if let Some(&dot) = points.first() {
                push_dot(&mut pieces, dot, style);
            }
            continue;
        }
        match style.dash {
            Some(dash) => {
                for piece in dashes(&points, run.closed, &dash) {
                    outline_open(&mut pieces, &piece, style);
                }
            }
            None if run.closed => append_pieces(&mut pieces, &points, style, true),
            None => outline_open(&mut pieces, &points, style),
        }
    }
    pieces
}

/// The whole stroke as one shape to fill with the non-zero rule.
pub fn outline_path(path: &Path, style: &StrokeStyle) -> Path {
    let mut union = Path::new();
    union.fill_rule = FillRule::NonZero;
    for piece in outline(path, style) {
        let points: Vec<Point> = piece
            .commands
            .iter()
            .filter_map(|command| match *command {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => Some(p),
                _ => None,
            })
            .collect();
        let forward = signed_area(&points) >= 0.0;
        let mut ordered = points.into_iter();
        let ordered: Vec<Point> = if forward {
            ordered.by_ref().collect()
        } else {
            ordered.rev().collect()
        };
        let Some((&first, rest)) = ordered.split_first() else {
            continue;
        };
        union.commands.push(PathCommand::MoveTo(first));
        union
            .commands
            .extend(rest.iter().map(|p| PathCommand::LineTo(*p)));
        union.commands.push(PathCommand::Close);
    }
    union
}

fn signed_area(points: &[Point]) -> f32 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

fn outline_open(pieces: &mut Vec<Path>, points: &[Point], style: &StrokeStyle) {
    if points.len() < 2 {
        return;
    }
    if style.cap == LineCap::Square {
        let mut extended = points.to_vec();
        extend_end(&mut extended, style.half, true);
        extend_end(&mut extended, style.half, false);
        append_pieces(pieces, &extended, style, false);
        return;
    }
    append_pieces(pieces, points, style, false);
    if style.cap == LineCap::Round {
        let last = points.len() - 1;
        push_round_cap(pieces, points[1], points[0], style.half);
        push_round_cap(pieces, points[last - 1], points[last], style.half);
    }
}

fn extend_end(points: &mut [Point], by: f32, first: bool) {
    let last = points.len() - 1;
    let (from, end) = if first { (1, 0) } else { (last - 1, last) };
    if let Some((dx, dy)) = unit(points[from], points[end]) {
        points[end] = Point::new(points[end].x + dx * by, points[end].y + dy * by);
    }
}

fn push_round_cap(pieces: &mut Vec<Path>, from: Point, end: Point, half: f32) {
    let Some((dx, dy)) = unit(from, end) else {
        return;
    };
    let facing = dy.atan2(dx);
    pieces.push(fan(end, half, facing - PI / 2.0, PI));
}

fn push_dot(pieces: &mut Vec<Path>, at: Point, style: &StrokeStyle) {
    let h = style.half;
    match style.cap {
        LineCap::Butt => {}
        LineCap::Round => pieces.push(fan(at, h, 0.0, 2.0 * PI)),
        LineCap::Square => pieces.push(polygon(&[
            Point::new(at.x - h, at.y - h),
            Point::new(at.x + h, at.y - h),
            Point::new(at.x + h, at.y + h),
            Point::new(at.x - h, at.y + h),
        ])),
    }
}

fn fan(center: Point, radius: f32, from: f32, sweep: f32) -> Path {
    let step = 2.0 * (1.0 - (0.05 / radius.max(0.05)).min(1.0)).acos();
    let steps = (sweep.abs() / step.max(1e-3)).ceil().clamp(4.0, 256.0);
    let mut ring = vec![center];
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = steps as u32;
    for i in 0..=count {
        #[allow(clippy::cast_precision_loss)]
        let angle = from + sweep * (i as f32) / steps;
        ring.push(Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    polygon(&ring)
}

fn dashes(points: &[Point], closed: bool, dash: &Dash) -> Vec<Vec<Point>> {
    let mut dasher = Dasher::new(dash, points[0]);
    let count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for i in 0..count {
        dasher.segment(points[i], points[(i + 1) % points.len()]);
    }
    dasher.finish()
}

struct Dasher<'d> {
    lengths: &'d [f32],
    index: usize,
    left: f32,
    current: Vec<Point>,
    out: Vec<Vec<Point>>,
}

impl<'d> Dasher<'d> {
    fn new(dash: &'d Dash, start: Point) -> Self {
        let lengths = dash.lengths();
        let period: f32 = lengths.iter().sum();
        let mut dasher = Self {
            lengths,
            index: 0,
            left: lengths[0],
            current: Vec::new(),
            out: Vec::new(),
        };
        let mut skip = dash.offset.rem_euclid(period);
        while skip >= dasher.left {
            skip -= dasher.left;
            dasher.advance();
        }
        dasher.left -= skip;
        if dasher.on() {
            dasher.current.push(start);
        }
        dasher
    }

    fn on(&self) -> bool {
        self.index.is_multiple_of(2)
    }

    fn advance(&mut self) {
        self.index = (self.index + 1) % self.lengths.len();
        self.left = self.lengths[self.index];
    }

    fn segment(&mut self, mut a: Point, b: Point) {
        let mut remaining = (b.x - a.x).hypot(b.y - a.y);
        while remaining > 0.0 {
            let step = self.left.min(remaining);
            let t = step / remaining;
            a = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            remaining -= step;
            self.left -= step;
            if self.on() {
                self.current.push(a);
            }
            if self.left <= 1e-6 {
                self.boundary(a);
            }
        }
    }

    fn boundary(&mut self, at: Point) {
        if self.on() {
            self.out.push(take(&mut self.current));
        }
        self.advance();
        if self.on() {
            self.current.push(at);
        }
    }

    fn finish(mut self) -> Vec<Vec<Point>> {
        if self.on() {
            self.out.push(take(&mut self.current));
        }
        self.out.retain(|piece| piece.len() >= 2);
        self.out
    }
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

fn append_pieces(pieces: &mut Vec<Path>, points: &[Point], style: &StrokeStyle, closed: bool) {
    let half = style.half;
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
            push_join(pieces, (points[i], next, following), style, outward);
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

fn push_join(
    pieces: &mut Vec<Path>,
    (previous, corner, following): (Point, Point, Point),
    style: &StrokeStyle,
    outward: f32,
) {
    let half = style.half;
    let (Some(n1), Some(n2)) = (
        normal_of(previous, corner, half),
        normal_of(corner, following, half),
    ) else {
        return;
    };
    let a = Point::new(corner.x + outward * n1.x, corner.y + outward * n1.y);
    let b = Point::new(corner.x + outward * n2.x, corner.y + outward * n2.y);
    match style.join {
        LineJoin::Round => {
            let from = (a.y - corner.y).atan2(a.x - corner.x);
            let to = (b.y - corner.y).atan2(b.x - corner.x);
            let mut sweep = to - from;
            if sweep > PI {
                sweep -= 2.0 * PI;
            } else if sweep < -PI {
                sweep += 2.0 * PI;
            }
            pieces.push(fan(corner, half, from, sweep));
        }
        LineJoin::Miter => match miter_tip(corner, (n1, n2), half, style.miter_limit) {
            Some((tx, ty)) => pieces.push(polygon(&[
                corner,
                a,
                Point::new(corner.x + outward * tx, corner.y + outward * ty),
                b,
            ])),
            None => pieces.push(polygon(&[corner, a, b])),
        },
        LineJoin::Bevel => pieces.push(polygon(&[corner, a, b])),
    }
}

fn miter_tip(
    _corner: Point,
    (n1, n2): (Point, Point),
    half: f32,
    limit: f32,
) -> Option<(f32, f32)> {
    let cos = (n1.x * n2.x + n1.y * n2.y) / (half * half);
    let denominator = 1.0 + cos;
    if denominator <= f32::EPSILON {
        return None;
    }
    if (2.0 / denominator).sqrt() > limit {
        return None;
    }
    Some(((n1.x + n2.x) / denominator, (n1.y + n2.y) / denominator))
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
        let pieces = outline(
            &line(&[(0.0, 10.0), (20.0, 10.0)]),
            &StrokeStyle::solid(4.0),
        );

        assert_eq!(pieces.len(), 1, "one segment, no joins");
        assert_eq!(points_of(&pieces).len(), 4, "a quad has four corners");
    }

    #[test]
    fn a_reversal_is_always_treated_as_sharp() {
        let pieces = outline(
            &line(&[(0.0, 0.0), (10.0, 0.0), (0.0, 0.0)]),
            &StrokeStyle::solid(2.0),
        );
        assert!(pieces.len() > 1, "the run must be cut at a reversal");
    }

    #[test]
    fn the_outline_is_as_wide_as_the_stroke() {
        let pieces = outline(&line(&[(0.0, 0.0), (0.0, 20.0)]), &StrokeStyle::solid(6.0));
        let (min_x, max_x, _, _) = bounds(&pieces);

        assert!(
            (max_x - min_x - 6.0).abs() < 1e-4,
            "width was {}",
            max_x - min_x
        );
    }

    #[test]
    fn the_outline_covers_the_length_of_the_line() {
        let pieces = outline(
            &line(&[(0.0, 10.0), (20.0, 10.0)]),
            &StrokeStyle::solid(2.0),
        );
        let (min_x, max_x, _, _) = bounds(&pieces);

        assert!((min_x - 0.0).abs() < 1e-4 && (max_x - 20.0).abs() < 1e-4);
    }

    #[test]
    fn a_sharp_bend_cuts_the_run_and_adds_a_wedge() {
        let pieces = outline(
            &line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]),
            &StrokeStyle::solid(2.0),
        );
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
        let pieces = outline(&line(&points), &StrokeStyle::solid(1.5));

        assert_eq!(
            pieces.len(),
            1,
            "nineteen near-straight segments, one piece"
        );
    }

    #[test]
    fn a_very_long_run_is_broken_into_pieces() {
        let points: Vec<(f32, f32)> = (0..200u8).map(|i| (f32::from(i) * 2.0, 50.0)).collect();
        let pieces = outline(&line(&points), &StrokeStyle::solid(1.5));

        assert!(pieces.len() > 1, "got {}", pieces.len());
        assert!(
            pieces.len() < 20,
            "but not one per segment: {}",
            pieces.len()
        );
    }

    #[test]
    fn repeated_points_are_dropped() {
        let pieces = outline(
            &line(&[(0.0, 0.0), (0.0, 0.0), (10.0, 0.0)]),
            &StrokeStyle::solid(2.0),
        );
        assert_eq!(pieces.len(), 1, "still one segment");
    }

    #[test]
    fn a_single_point_strokes_to_nothing() {
        assert!(outline(&line(&[(5.0, 5.0)]), &StrokeStyle::solid(2.0)).is_empty());
    }

    #[test]
    fn an_empty_path_strokes_to_nothing() {
        assert!(outline(&Path::new(), &StrokeStyle::solid(2.0)).is_empty());
    }

    #[test]
    fn a_zero_width_stroke_still_has_some_body() {
        let pieces = outline(&line(&[(0.0, 0.0), (10.0, 0.0)]), &StrokeStyle::solid(0.0));
        assert!(!pieces.is_empty());
        let (_, _, min_y, max_y) = bounds(&pieces);
        assert!(max_y > min_y, "it should still have a thickness");
    }

    #[test]
    fn a_closed_run_strokes_all_the_way_round() {
        let mut path = line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        path.commands.push(PathCommand::Close);

        let pieces = outline(&path, &StrokeStyle::solid(2.0));
        assert_eq!(pieces.len(), 4 + 4, "four sides and four corners");
    }

    #[test]
    fn separate_subpaths_stroke_separately() {
        let mut path = line(&[(0.0, 0.0), (10.0, 0.0)]);
        path.commands
            .push(PathCommand::MoveTo(Point::new(0.0, 20.0)));
        path.commands
            .push(PathCommand::LineTo(Point::new(10.0, 20.0)));

        assert_eq!(outline(&path, &StrokeStyle::solid(2.0)).len(), 2);
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

    fn styled(width: f32, paint: Paint) -> StrokeStyle {
        StrokeStyle::of(&paint.stroke_width(width))
    }

    #[test]
    fn caps_end_where_they_should() {
        let path = line(&[(0.0, 10.0), (20.0, 10.0)]);
        let ends = |cap| {
            let (x0, x1, _, _) = bounds(&outline(&path, &styled(4.0, Paint::new().line_cap(cap))));
            (x0, x1)
        };
        assert_eq!(ends(LineCap::Butt), (0.0, 20.0));
        assert_eq!(ends(LineCap::Square), (-2.0, 22.0));
        let (x0, x1) = ends(LineCap::Round);
        assert!(
            (x0 + 2.0).abs() < 0.06 && (x1 - 22.0).abs() < 0.06,
            "{x0}, {x1}"
        );
    }

    #[test]
    fn a_miter_reaches_its_tip_and_bevels_past_the_limit() {
        let v = line(&[(0.0, 0.0), (20.0, 0.0), (10.0, 17.320_508)]);
        let top = |paint: Paint| bounds(&outline(&v, &styled(2.0, paint))).1;
        let mitred = top(Paint::new());
        let beveled = top(Paint::new().miter_limit(1.5));
        let cut = top(Paint::new().line_join(LineJoin::Bevel));
        assert!(
            mitred > beveled + 0.5,
            "the miter sticks out: {mitred} vs {beveled}"
        );
        assert_eq!(beveled, cut, "past the limit it is a bevel");
    }

    #[test]
    fn a_round_join_stays_within_half_the_width_of_the_corner() {
        let v = line(&[(0.0, 0.0), (20.0, 0.0), (10.0, 17.320_508)]);
        let pieces = outline(&v, &styled(4.0, Paint::new().line_join(LineJoin::Round)));
        let corner = Point::new(20.0, 0.0);
        let far = points_of(&pieces)
            .iter()
            .filter(|p| p.x > 19.0)
            .map(|p| (p.x - corner.x).hypot(p.y - corner.y))
            .fold(0.0f32, f32::max);
        assert!(far <= 2.0 + 1e-3, "got {far}");
    }

    #[test]
    fn dashes_leave_gaps_and_start_at_the_offset() {
        let path = line(&[(0.0, 0.0), (20.0, 0.0)]);
        let dashed = |offset| outline(&path, &styled(2.0, Paint::new().dash(&[5.0, 5.0], offset)));
        assert_eq!(dashed(0.0).len(), 2, "on 0..5 and 10..15");
        let (x0, _, _, _) = bounds(&dashed(5.0));
        assert!(
            (x0 - 5.0).abs() < 1e-3,
            "starts in a gap, so the first dash is at 5: {x0}"
        );
        let pieces = dashed(2.5);
        assert_eq!(pieces.len(), 3, "a part dash, a whole one, a part");
    }

    #[test]
    fn an_odd_pattern_repeats_so_dashes_and_gaps_alternate() {
        let dash = Dash::new(&[3.0], 0.0).expect("pattern");
        assert_eq!(dash.lengths(), &[3.0, 3.0]);
        assert!(
            Dash::new(&[0.0, 0.0], 0.0).is_none(),
            "nothing to draw is solid"
        );
    }

    #[test]
    fn a_closed_dashed_shape_dashes_its_closing_edge_too() {
        let mut square = line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        square.commands.push(PathCommand::Close);
        let pieces = outline(&square, &styled(1.0, Paint::new().dash(&[2.0, 2.0], 0.0)));
        let left = points_of(&pieces)
            .iter()
            .any(|p| p.x < 0.6 && p.y > 1.0 && p.y < 9.0);
        assert!(
            left,
            "the left edge, which only the close draws, has dashes"
        );
    }

    #[test]
    fn a_lone_point_with_a_round_cap_is_a_dot() {
        let dot = line(&[(5.0, 5.0), (5.0, 5.0)]);
        let pieces = outline(&dot, &styled(4.0, Paint::new().line_cap(LineCap::Round)));
        let (x0, x1, y0, y1) = bounds(&pieces);
        assert!((x1 - x0 - 4.0).abs() < 0.11 && (y1 - y0 - 4.0).abs() < 0.11);
        assert!(
            outline(&dot, &styled(4.0, Paint::new())).is_empty(),
            "butt draws nothing"
        );
    }

    #[test]
    fn the_joined_outline_winds_every_piece_one_way() {
        let v = line(&[(0.0, 0.0), (20.0, 0.0), (10.0, 17.320_508), (30.0, 30.0)]);
        let style = styled(
            4.0,
            Paint::new()
                .line_join(LineJoin::Round)
                .line_cap(LineCap::Round),
        );
        let union = outline_path(&v, &style);
        assert_eq!(union.fill_rule, FillRule::NonZero);
        let mut rings: Vec<Vec<Point>> = Vec::new();
        for command in &union.commands {
            match *command {
                PathCommand::MoveTo(p) => rings.push(vec![p]),
                PathCommand::LineTo(p) => rings.last_mut().expect("a ring").push(p),
                _ => {}
            }
        }
        assert!(rings.len() > 3, "pieces, joins and caps");
        assert!(rings.iter().all(|ring| signed_area(ring) >= 0.0));
    }
}
