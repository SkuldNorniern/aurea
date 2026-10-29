//! Turning a stroked path into something the filler can fill.

use super::path::{cubic_steps, quad_steps};
use crate::types::{Dash, FillRule, LineCap, LineJoin, Paint, Path, PathCommand, Point};
use std::cell::RefCell;
use std::f32::consts::PI;
use std::mem::take;

/// How a path is stroked: half its width, its ends, corners and dashes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    /// Half the stroke width.
    pub half: f32,
    /// End style of open strokes.
    pub cap: LineCap,
    /// Corner style.
    pub join: LineJoin,
    /// Miter limit.
    pub miter_limit: f32,
    /// Dash pattern, if any.
    pub dash: Option<Dash>,
}

impl StrokeStyle {
    /// The style for `paint`.
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

thread_local! {
    /// Reused by every stroke on this thread, so stroking does not allocate per stroke.
    static STROKE: RefCell<Scratch> = RefCell::new(Scratch::default());
}

/// Frees this thread's stroke buffers; the next stroke grows them again.
pub(crate) fn trim_scratch() {
    STROKE.with_borrow_mut(|s| *s = Scratch::default());
}

/// The buffers a stroke builds in, kept from one stroke to the next.
#[derive(Default)]
struct Scratch {
    /// The points of every run, back to back.
    points: Vec<Point>,
    runs: Vec<Run>,
    /// An open run with square caps added.
    extended: Vec<Point>,
    /// The run `append_pieces` is cutting.
    run: Vec<Point>,
    /// The ring being emitted.
    ring: Vec<Point>,
    dash: DashRuns,
}

/// A run of points from one `MoveTo` to the next, with whether it closed. Its points are
/// `Scratch::points[start..end]`.
struct Run {
    start: usize,
    end: usize,
    closed: bool,
}

/// Where a stroke's rings go, and the buffers they are built in.
struct Out<'a> {
    sink: &'a mut dyn FnMut(&[Point]),
    ring: &'a mut Vec<Point>,
    run: &'a mut Vec<Point>,
    extended: &'a mut Vec<Point>,
}

/// The whole stroke as one shape to fill with the non-zero rule.
pub fn outline_path(path: &Path, style: &StrokeStyle) -> Path {
    let mut union = Path::new();
    union.fill_rule = FillRule::NonZero;
    STROKE.with_borrow_mut(|s| {
        stroke(path, style, s, &mut |ring: &[Point]| {
            join_ring(&mut union, ring)
        });
    });
    union
}

/// The outline of `path` stroked in `style`, as convex pieces to fill.
#[cfg(test)]
fn outline(path: &Path, style: &StrokeStyle) -> Vec<Path> {
    let mut pieces = Vec::new();
    STROKE.with_borrow_mut(|s| {
        stroke(path, style, s, &mut |ring: &[Point]| {
            pieces.push(polygon(ring))
        });
    });
    pieces
}

/// Calls `sink` with each closed ring of the stroke, in order.
fn stroke(path: &Path, style: &StrokeStyle, s: &mut Scratch, sink: &mut dyn FnMut(&[Point])) {
    let Scratch {
        points,
        runs,
        extended,
        run,
        ring,
        dash,
    } = s;
    flatten_into(path, points, runs);
    dedup_runs(points, runs);
    let mut out = Out {
        sink,
        ring,
        run,
        extended,
    };
    for r in runs.iter() {
        let run_points = &points[r.start..r.end];
        if run_points.len() < 2 {
            if let Some(&dot) = run_points.first() {
                push_dot(&mut out, dot, style);
            }
            continue;
        }
        match style.dash {
            Some(pattern) => {
                dashes(run_points, r.closed, &pattern, dash);
                for piece in dash.iter() {
                    outline_open(&mut out, piece, style);
                }
            }
            None if r.closed => append_pieces(&mut out, run_points, style, true),
            None => outline_open(&mut out, run_points, style),
        }
    }
}

/// Splits a path into runs of straight points in `points`, subdividing any curves.
fn flatten_into(path: &Path, points: &mut Vec<Point>, runs: &mut Vec<Run>) {
    points.clear();
    runs.clear();
    let mut start = 0;
    let mut cursor = Point::new(0.0, 0.0);

    for command in &path.commands {
        match command {
            PathCommand::MoveTo(p) => {
                push_run(points, runs, &mut start, false);
                cursor = *p;
                points.push(cursor);
            }
            PathCommand::LineTo(p) => {
                cursor = *p;
                points.push(cursor);
            }
            PathCommand::QuadTo(c, p) => {
                let steps = quad_steps(cursor, *c, *p);
                for i in 1..=steps {
                    let t = f32::from(i) / f32::from(steps);
                    points.push(quadratic(cursor, *c, *p, t));
                }
                cursor = *p;
            }
            PathCommand::CubicTo(c1, c2, p) => {
                let steps = cubic_steps(cursor, *c1, *c2, *p);
                for i in 1..=steps {
                    let t = f32::from(i) / f32::from(steps);
                    points.push(cubic(cursor, *c1, *c2, *p, t));
                }
                cursor = *p;
            }
            PathCommand::Close => {
                if let Some(first) = points[start..].first().copied() {
                    cursor = first;
                }
                push_run(points, runs, &mut start, true);
            }
        }
    }
    push_run(points, runs, &mut start, false);
}

/// Keeps the run from `start` if it has two points, otherwise drops it.
fn push_run(points: &mut Vec<Point>, runs: &mut Vec<Run>, start: &mut usize, closed: bool) {
    if points.len() - *start >= 2 {
        runs.push(Run {
            start: *start,
            end: points.len(),
            closed,
        });
    } else {
        points.truncate(*start);
    }
    *start = points.len();
}

/// Drops each run's repeated points in place, and a closing point that repeats its start.
fn dedup_runs(points: &mut [Point], runs: &mut [Run]) {
    let mut w = 0;
    for run in runs.iter_mut() {
        let start = w;
        for i in run.start..run.end {
            let p = points[i];
            if w == start || !near(points[w - 1], p) {
                points[w] = p;
                w += 1;
            }
        }
        if run.closed && w - start > 2 && near(points[start], points[w - 1]) {
            w -= 1;
        }
        run.start = start;
        run.end = w;
    }
}

/// Joins one ring into `union`, wound the same way as every other.
fn join_ring(union: &mut Path, ring: &[Point]) {
    if ring.is_empty() {
        return;
    }
    let n = ring.len();
    let forward = signed_area(ring) >= 0.0;
    let at = |i: usize| if forward { ring[i] } else { ring[n - 1 - i] };
    union.commands.push(PathCommand::MoveTo(at(0)));
    union
        .commands
        .extend((1..n).map(|i| PathCommand::LineTo(at(i))));
    union.commands.push(PathCommand::Close);
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

fn send(out: &mut Out<'_>, ring: &[Point]) {
    (out.sink)(ring);
}

fn outline_open(out: &mut Out<'_>, points: &[Point], style: &StrokeStyle) {
    if points.len() < 2 {
        return;
    }
    if style.cap == LineCap::Square {
        let mut extended = take(&mut *out.extended);
        extended.clear();
        extended.extend_from_slice(points);
        extend_end(&mut extended, style.half, true);
        extend_end(&mut extended, style.half, false);
        append_pieces(out, &extended, style, false);
        *out.extended = extended;
        return;
    }
    append_pieces(out, points, style, false);
    if style.cap == LineCap::Round {
        let last = points.len() - 1;
        push_round_cap(out, points[1], points[0], style.half);
        push_round_cap(out, points[last - 1], points[last], style.half);
    }
}

fn extend_end(points: &mut [Point], by: f32, first: bool) {
    let last = points.len() - 1;
    let (from, end) = if first { (1, 0) } else { (last - 1, last) };
    if let Some((dx, dy)) = unit(points[from], points[end]) {
        points[end] = Point::new(points[end].x + dx * by, points[end].y + dy * by);
    }
}

fn push_round_cap(out: &mut Out<'_>, from: Point, end: Point, half: f32) {
    let Some((dx, dy)) = unit(from, end) else {
        return;
    };
    let facing = dy.atan2(dx);
    fan(out, end, half, facing - PI / 2.0, PI);
}

fn push_dot(out: &mut Out<'_>, at: Point, style: &StrokeStyle) {
    let h = style.half;
    match style.cap {
        LineCap::Butt => {}
        LineCap::Round => fan(out, at, h, 0.0, 2.0 * PI),
        LineCap::Square => send(
            out,
            &[
                Point::new(at.x - h, at.y - h),
                Point::new(at.x + h, at.y - h),
                Point::new(at.x + h, at.y + h),
                Point::new(at.x - h, at.y + h),
            ],
        ),
    }
}

fn fan(out: &mut Out<'_>, center: Point, radius: f32, from: f32, sweep: f32) {
    let step = 2.0 * (1.0 - (0.05 / radius.max(0.05)).min(1.0)).acos();
    let steps = (sweep.abs() / step.max(1e-3)).ceil().clamp(4.0, 256.0);
    let ring = &mut *out.ring;
    ring.clear();
    ring.push(center);
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
    (out.sink)(ring.as_slice());
}

/// The dashed runs of one subpath: their points back to back, and where each run starts and ends
/// in `points`.
#[derive(Default)]
struct DashRuns {
    points: Vec<Point>,
    starts: Vec<usize>,
    ends: Vec<usize>,
}

impl DashRuns {
    fn clear(&mut self) {
        self.points.clear();
        self.starts.clear();
        self.ends.clear();
    }

    fn iter(&self) -> impl Iterator<Item = &[Point]> {
        self.starts
            .iter()
            .zip(&self.ends)
            .map(|(&start, &end)| &self.points[start..end])
    }
}

/// Splits `points` into the on-runs of `dash` into `runs`, which is cleared first.
fn dashes(points: &[Point], closed: bool, dash: &Dash, runs: &mut DashRuns) {
    runs.clear();
    let mut dasher = Dasher::new(dash, points[0], runs);
    let count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for i in 0..count {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        if dash.along_edges {
            dasher.restart(edge_phase(dash, a, b), a);
        }
        dasher.segment(a, b);
    }
    dasher.finish();
}

/// Where the pattern stands at `a` on the edge to `b`: `offset` plus `a` along the edge's own
/// axis, mirrored when it runs left or up so the on runs keep their screen places.
fn edge_phase(dash: &Dash, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (at, forward) = if dx.abs() >= dy.abs() {
        (a.x, dx >= 0.0)
    } else {
        (a.y, dy >= 0.0)
    };
    if forward {
        dash.offset + at
    } else {
        dash.lengths()[0] - dash.offset - at
    }
}

struct Dasher<'d> {
    lengths: &'d [f32],
    period: f32,
    index: usize,
    left: f32,
    /// Where the open run starts in `runs.points`.
    open: usize,
    runs: &'d mut DashRuns,
}

impl<'d> Dasher<'d> {
    fn new(dash: &'d Dash, start: Point, runs: &'d mut DashRuns) -> Self {
        let lengths = dash.lengths();
        let period: f32 = lengths.iter().sum();
        let mut dasher = Self {
            lengths,
            period,
            index: 0,
            left: lengths[0],
            open: 0,
            runs,
        };
        let phase = if dash.anchored {
            dash.offset + start.x + start.y
        } else {
            dash.offset
        };
        dasher.seat(phase, start);
        dasher
    }

    /// Ends the run in progress at `at` and stands the pattern at `phase` from there.
    fn restart(&mut self, phase: f32, at: Point) {
        if self.on() {
            self.close_run();
        }
        (self.index, self.left) = (0, self.lengths[0]);
        self.seat(phase, at);
    }

    /// Moves `phase` into the pattern from its start, opening a run at `at` when that is on.
    fn seat(&mut self, phase: f32, at: Point) {
        let mut skip = phase.rem_euclid(self.period);
        while skip >= self.left {
            skip -= self.left;
            self.advance();
        }
        self.left -= skip;
        if self.on() {
            self.open_run(at);
        }
    }

    fn on(&self) -> bool {
        self.index.is_multiple_of(2)
    }

    fn advance(&mut self) {
        self.index = (self.index + 1) % self.lengths.len();
        self.left = self.lengths[self.index];
    }

    fn open_run(&mut self, at: Point) {
        self.open = self.runs.points.len();
        self.runs.points.push(at);
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
                self.runs.points.push(a);
            }
            if self.left <= 1e-6 {
                self.boundary(a);
            }
        }
    }

    fn boundary(&mut self, at: Point) {
        if self.on() {
            self.close_run();
        }
        self.advance();
        if self.on() {
            self.open_run(at);
        }
    }

    /// Keeps the open run if it has two points, otherwise drops it.
    fn close_run(&mut self) {
        let end = self.runs.points.len();
        if end - self.open >= 2 {
            self.runs.starts.push(self.open);
            self.runs.ends.push(end);
        } else {
            self.runs.points.truncate(self.open);
        }
    }

    fn finish(mut self) {
        if self.on() {
            self.close_run();
        }
    }
}

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
}

const MAX_RUN: usize = 48;

fn append_pieces(out: &mut Out<'_>, points: &[Point], style: &StrokeStyle, closed: bool) {
    let half = style.half;
    let segments = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    if segments == 0 {
        return;
    }

    let mut run = take(&mut *out.run);
    run.clear();
    run.push(points[0]);
    for i in 0..segments {
        let next = points[(i + 1) % points.len()];
        run.push(next);

        let is_last = i + 1 == segments;
        if is_last && !closed {
            continue;
        }

        let following = following_of(points, i);
        if let Some(outward) = wedge_side(points[i], next, following, half) {
            outline_run(out, &run, half);
            push_join(out, (points[i], next, following), style, outward);
            run.clear();
            run.push(next);
        } else if run.len() >= MAX_RUN {
            outline_run(out, &run, half);
            run.clear();
            run.push(next);
        }
    }
    outline_run(out, &run, half);
    *out.run = run;
}

fn following_of(points: &[Point], i: usize) -> Point {
    points[(i + 2) % points.len()]
}

/// Emits one ring around `run`: its left side forward, then its right side back.
fn outline_run(out: &mut Out<'_>, run: &[Point], half: f32) {
    if run.len() < 2 {
        return;
    }
    let ring = &mut *out.ring;
    ring.clear();
    for pair in run.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let Some(n) = normal_of(a, b, half) else {
            continue;
        };
        ring.push(Point::new(a.x + n.x, a.y + n.y));
        ring.push(Point::new(b.x + n.x, b.y + n.y));
    }
    if ring.is_empty() {
        return;
    }
    for pair in run.windows(2).rev() {
        let (a, b) = (pair[0], pair[1]);
        let Some(n) = normal_of(a, b, half) else {
            continue;
        };
        ring.push(Point::new(b.x - n.x, b.y - n.y));
        ring.push(Point::new(a.x - n.x, a.y - n.y));
    }
    (out.sink)(ring.as_slice());
}

fn push_join(
    out: &mut Out<'_>,
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
            fan(out, corner, half, from, sweep);
        }
        LineJoin::Miter => match miter_tip(corner, (n1, n2), half, style.miter_limit) {
            Some((tx, ty)) => send(
                out,
                &[
                    corner,
                    a,
                    Point::new(corner.x + outward * tx, corner.y + outward * ty),
                    b,
                ],
            ),
            None => send(out, &[corner, a, b]),
        },
        LineJoin::Bevel => send(out, &[corner, a, b]),
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

#[cfg(test)]
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

        let (mut points, mut runs) = (Vec::new(), Vec::new());
        flatten_into(&path, &mut points, &mut runs);
        assert_eq!(runs.len(), 1);
        assert!(
            runs[0].end - runs[0].start > 2,
            "the curve should be flattened"
        );
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

    /// The stroker's dashing before `DashRuns`, to pin unanchored output.
    fn legacy_dashes(points: &[Point], closed: bool, dash: &Dash) -> Vec<Vec<Point>> {
        struct Old<'d> {
            lengths: &'d [f32],
            index: usize,
            left: f32,
            current: Vec<Point>,
            out: Vec<Vec<Point>>,
        }
        impl<'d> Old<'d> {
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

        let mut dasher = Old::new(dash, points[0]);
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

    /// The x extent of each dashed run of `points`, subpaths in order.
    fn dash_spans(subpaths: &[&[Point]], dash: &Dash) -> Vec<(f32, f32)> {
        let mut runs = DashRuns::default();
        let mut spans = Vec::new();
        for subpath in subpaths {
            dashes(subpath, false, dash, &mut runs);
            spans.extend(runs.iter().map(|run| (run[0].x, run[run.len() - 1].x)));
        }
        spans
    }

    #[test]
    fn unanchored_dashes_match_the_old_stroker() {
        let zigzag: Vec<Point> = [
            (0.0, 0.0),
            (13.0, 0.5),
            (13.0, 9.0),
            (-4.0, 9.0),
            (-4.0, 0.0),
        ]
        .iter()
        .map(|&(x, y)| Point::new(x, y))
        .collect();
        let mut runs = DashRuns::default();
        let patterns: [(&[f32], f32); 4] = [
            (&[5.0, 5.0], 0.0),
            (&[3.0, 1.0, 2.0], 2.5),
            (&[1.0, 4.0], -7.0),
            (&[0.5, 0.25], 100.0),
        ];
        for (lengths, offset) in patterns {
            let dash = Dash::new(lengths, offset).expect("pattern");
            for closed in [false, true] {
                dashes(&zigzag, closed, &dash, &mut runs);
                let new: Vec<Vec<Point>> = runs.iter().map(<[Point]>::to_vec).collect();
                assert_eq!(new, legacy_dashes(&zigzag, closed, &dash));
            }
        }
    }

    #[test]
    fn dashes_along_edges_sit_in_the_same_screen_places_on_every_side() {
        // a clockwise square: right along the top, down, left along the bottom, up
        let square = [
            Point::new(1.0, 2.0),
            Point::new(31.0, 2.0),
            Point::new(31.0, 32.0),
            Point::new(1.0, 32.0),
        ];
        let dash = Dash::new(&[4.0, 3.0], 2.5).expect("pattern").along_edges();
        let mut runs = DashRuns::default();
        dashes(&square, true, &dash, &mut runs);
        // each run as its span along its edge's axis, on the given side
        let spans = |on: &dyn Fn(Point) -> bool, axis: fn(Point) -> f32| {
            let hundredths = |v: f32| (v * 100.0).round() / 100.0;
            let mut spans: Vec<(f32, f32)> = runs
                .iter()
                .filter(|run| run.iter().all(|p| on(*p)))
                .map(|run| {
                    let (a, b) = (axis(run[0]), axis(run[run.len() - 1]));
                    (hundredths(a.min(b)), hundredths(a.max(b)))
                })
                .collect();
            spans.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            spans
        };
        let top = spans(&|p| p.y == 2.0, |p| p.x);
        let bottom = spans(&|p| p.y == 32.0, |p| p.x);
        let left = spans(&|p| p.x == 1.0, |p| p.y);
        let right = spans(&|p| p.x == 31.0, |p| p.y);
        assert!(!top.is_empty() && !left.is_empty());
        // on runs sit where (offset + x) mod 7 < 4, whichever way the edge goes
        let inner = |spans: &[(f32, f32)]| {
            spans
                .iter()
                .filter(|(a, b)| *a > 1.5 && *b < 30.5)
                .copied()
                .collect::<Vec<_>>()
        };
        assert_eq!(inner(&top), inner(&bottom), "top and bottom");
        assert_eq!(inner(&left), inner(&right), "left and right");
        for (a, _) in inner(&top) {
            let phase = (2.5 + a).rem_euclid(7.0);
            assert!(
                phase < 0.01 || phase > 6.99,
                "a dash starts on the grid: {a} {phase}"
            );
        }
    }

    #[test]
    fn anchored_subpaths_dash_on_the_same_grid() {
        let a = [Point::new(0.0, 0.0), Point::new(12.0, 0.0)];
        let b = [Point::new(7.0, 0.0), Point::new(20.0, 0.0)];
        let whole = [Point::new(0.0, 0.0), Point::new(20.0, 0.0)];
        let plain = Dash::new(&[4.0, 4.0], 0.0).expect("pattern");
        let anchored = plain.anchored();

        assert_eq!(
            dash_spans(&[&whole], &anchored),
            vec![(0.0, 4.0), (8.0, 12.0), (16.0, 20.0)],
            "one line"
        );
        assert_eq!(
            dash_spans(&[&a, &b], &anchored),
            vec![(0.0, 4.0), (8.0, 12.0), (8.0, 12.0), (16.0, 20.0)],
            "the second subpath joins the grid"
        );
        assert_eq!(
            dash_spans(&[&a, &b], &plain),
            vec![(0.0, 4.0), (8.0, 12.0), (7.0, 11.0), (15.0, 19.0)],
            "unanchored, the second starts its pattern at 0"
        );
    }

    /// FNV-1a over the RGBA bytes, stable across toolchains.
    fn fnv1a(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// Strokes that exercise every branch of the stroker, each with its canvas scale.
    fn golden_cases() -> Vec<(Path, Paint, f32)> {
        use crate::PaintStyle;
        let stroke = || Paint::new().style(PaintStyle::Stroke);
        let closed_box = {
            let mut path = line(&[(130.0, 10.0), (190.0, 10.0), (190.0, 70.0), (130.0, 70.0)]);
            path.commands.push(PathCommand::Close);
            path
        };
        let edged = stroke()
            .stroke_width(2.0)
            .line_join(LineJoin::Round)
            .dash(&[5.0, 3.0], 1.0);
        let edged = Paint {
            dash: Dash::new(&[5.0, 3.0], 1.0).map(|d| d.along_edges()),
            ..edged
        };
        let curve = {
            let mut path = Path::new();
            path.commands
                .push(PathCommand::MoveTo(Point::new(4.0, 60.0)));
            path.commands.push(PathCommand::CubicTo(
                Point::new(30.0, 0.0),
                Point::new(60.0, 120.0),
                Point::new(90.0, 60.0),
            ));
            path
        };
        vec![
            (
                line(&[(10.0, 10.0), (60.0, 12.0), (30.0, 50.0), (90.0, 30.0)]),
                stroke().stroke_width(4.0),
                1.0,
            ),
            (
                line(&[(20.0, 100.0), (70.0, 100.0), (95.0, 80.0)]),
                stroke()
                    .stroke_width(6.0)
                    .line_cap(LineCap::Round)
                    .line_join(LineJoin::Round),
                1.0,
            ),
            (
                line(&[(8.0, 140.0), (60.0, 140.0)]),
                stroke().stroke_width(5.0).line_cap(LineCap::Square),
                1.0,
            ),
            (
                line(&[(5.0, 170.0), (120.0, 170.0), (120.0, 190.0)]),
                stroke()
                    .stroke_width(3.0)
                    .line_cap(LineCap::Round)
                    .dash(&[6.0, 4.0], 2.5),
                1.0,
            ),
            (closed_box, edged, 1.0),
            (
                curve,
                stroke()
                    .stroke_width(3.0)
                    .line_cap(LineCap::Round)
                    .line_join(LineJoin::Round),
                1.0,
            ),
            (
                line(&[(100.0, 120.0), (100.0, 120.0), (120.0, 124.0)]),
                stroke().stroke_width(4.0).line_cap(LineCap::Round),
                1.0,
            ),
            (
                line(&[(150.0, 150.0), (150.0, 150.0)]),
                stroke().stroke_width(4.0).line_cap(LineCap::Round),
                1.0,
            ),
            (
                line(&[(4.0, 180.0), (60.0, 186.0)]),
                stroke().stroke_width(0.3),
                1.0,
            ),
            (
                line(&[(4.0, 180.0), (60.0, 186.0)]),
                stroke().stroke_width(0.3),
                1.5,
            ),
            (
                line(&[(10.0, 100.0), (90.0, 60.0)]),
                stroke().stroke_width(40.0),
                1.0,
            ),
        ]
    }

    #[test]
    fn strokes_render_the_same_pixels() {
        use crate::CpuRasterizer;
        let expected: [u64; 11] = [
            2362692873945308036,
            9567466158870693164,
            13933877095181122597,
            7841828774547950660,
            11992003363753275224,
            16126321927536331494,
            8324095631378301055,
            14833076215688421957,
            11650722776545918005,
            8278279517010211061,
            15797700667854759237,
        ];
        let actual: Vec<u64> = golden_cases()
            .iter()
            .map(|(path, paint, scale)| {
                let pixels = CpuRasterizer::draw_offscreen(200, 200, *scale, |ctx| {
                    ctx.draw_path(path, paint)
                })
                .expect("draw");
                fnv1a(&pixels.rgba)
            })
            .collect();
        assert_eq!(actual, expected, "hashes, in case order");
    }
}
