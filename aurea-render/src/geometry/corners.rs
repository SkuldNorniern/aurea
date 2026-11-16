//! Corner radii and smoothing for rounded shapes, and the outline they make.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, SQRT_2};

use super::{FillRule, Path, PathCommand, Point, Rect};

/// A radius for each corner, and how smoothly each corner runs into its edges.
///
/// `smoothing` 0 is a circular corner; above that the curve starts further along the edge and
/// eases in, as Apple's continuous corners do (about 0.6). The construction is Figma's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners {
    /// Top left radius.
    pub top_left: f32,
    /// Top right radius.
    pub top_right: f32,
    /// Bottom right radius.
    pub bottom_right: f32,
    /// Bottom left radius.
    pub bottom_left: f32,
    /// How smoothly the corners ease into the edges, from 0 (circular) to 1.
    pub smoothing: f32,
}

impl Corners {
    /// Every corner `radius`, circular.
    pub const fn all(radius: f32) -> Self {
        Self::new(radius, radius, radius, radius)
    }

    /// Each corner its own radius, clockwise from the top left, circular.
    pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
            smoothing: 0.0,
        }
    }

    /// The same radii with corners smoothed by `smoothing`, from 0 to 1.
    #[must_use]
    pub fn smooth(mut self, smoothing: f32) -> Self {
        self.smoothing = smoothing.clamp(0.0, 1.0);
        self
    }
}

/// One corner's outline in its own frame: `s` back along the edge coming in, `t` along the edge
/// going out, both from the corner point.
struct Corner {
    /// Where the curve leaves the edge coming in, as `s`.
    p: f32,
    radius: f32,
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    /// The circular part's sweep, in radians.
    sweep: f32,
}

impl Corner {
    /// `radius` smoothed by `smoothing`, kept within `limit` of the corner along each edge.
    fn new(radius: f32, smoothing: f32, limit: f32) -> Self {
        let (mut radius, mut smoothing) = (radius.max(0.0), smoothing.clamp(0.0, 1.0));
        if (1.0 + smoothing) * radius > limit {
            // less smoothing first, then a smaller radius
            smoothing = if radius > 0.0 {
                limit / radius - 1.0
            } else {
                0.0
            };
            if smoothing < 0.0 {
                radius = limit.max(0.0);
                smoothing = 0.0;
            }
        }
        let p = (1.0 + smoothing) * radius;
        let sweep = FRAC_PI_2 * (1.0 - smoothing);
        let arc = (sweep / 2.0).sin() * radius * SQRT_2;
        let alpha = (FRAC_PI_2 - sweep) / 2.0;
        let to_arc = radius * (alpha / 2.0).tan();
        let beta = FRAC_PI_4 * smoothing;
        let c = to_arc * beta.cos();
        let d = c * beta.tan();
        let b = (p - arc - c - d) / 3.0;
        Self {
            p,
            radius,
            a: 2.0 * b,
            b,
            c,
            d,
            sweep,
        }
    }

    /// Adds the corner at `at`, the edge coming in along `u` and going out along `v`, after a line
    /// to where the curve starts unless `first`.
    fn push(&self, path: &mut Path, at: Point, u: (f32, f32), v: (f32, f32), first: bool) {
        let world = |s: f32, t: f32| Point::new(at.x - u.0 * s + v.0 * t, at.y - u.1 * s + v.1 * t);
        let start = world(self.p, 0.0);
        path.commands.push(if first {
            PathCommand::MoveTo(start)
        } else {
            PathCommand::LineTo(start)
        });
        if self.radius <= 0.0 {
            return;
        }
        let Self { p, a, b, c, d, .. } = *self;
        let from = world(p - a - b - c, d);
        path.commands.push(PathCommand::CubicTo(
            world(p - a, 0.0),
            world(p - a - b, 0.0),
            from,
        ));
        let to = world(d, p - a - b - c);
        let center = world(self.radius, self.radius);
        let k = 4.0 / 3.0 * (self.sweep / 4.0).tan() * self.radius;
        let tangent = |point: Point| {
            let (rx, ry) = (point.x - center.x, point.y - center.y);
            let len = rx.hypot(ry).max(f32::EPSILON);
            let (tx, ty) = (-ry / len, rx / len);
            // the way the corner runs
            if tx * (to.x - from.x) + ty * (to.y - from.y) < 0.0 {
                (-tx, -ty)
            } else {
                (tx, ty)
            }
        };
        let (t0, t1) = (tangent(from), tangent(to));
        path.commands.push(PathCommand::CubicTo(
            Point::new(from.x + t0.0 * k, from.y + t0.1 * k),
            Point::new(to.x - t1.0 * k, to.y - t1.1 * k),
            to,
        ));
        path.commands.push(PathCommand::CubicTo(
            world(0.0, p - a - b),
            world(0.0, p - a),
            world(0.0, p),
        ));
    }
}

impl Path {
    /// The outline of `rect` with `corners`, each kept to half the shorter side.
    pub fn rounded(rect: Rect, corners: Corners) -> Self {
        let mut path = Self::new();
        path.push_rounded(rect, corners);
        path
    }

    /// Adds [`Path::rounded`]'s outline to this path.
    pub fn push_rounded(&mut self, rect: Rect, corners: Corners) {
        let limit = rect.width.min(rect.height).max(0.0) / 2.0;
        let (x0, y0, x1, y1) = (rect.x, rect.y, rect.right(), rect.bottom());
        let s = corners.smoothing;
        let each = [
            (
                corners.top_right,
                Point::new(x1, y0),
                (1.0, 0.0),
                (0.0, 1.0),
            ),
            (
                corners.bottom_right,
                Point::new(x1, y1),
                (0.0, 1.0),
                (-1.0, 0.0),
            ),
            (
                corners.bottom_left,
                Point::new(x0, y1),
                (-1.0, 0.0),
                (0.0, -1.0),
            ),
            (
                corners.top_left,
                Point::new(x0, y0),
                (0.0, -1.0),
                (1.0, 0.0),
            ),
        ];
        for (i, (radius, at, u, v)) in each.into_iter().enumerate() {
            Corner::new(radius, s, limit).push(self, at, u, v, i == 0);
        }
        self.commands.push(PathCommand::Close);
    }
}

impl Path {
    /// The band `width` wide just inside [`Path::rounded`]'s outline, filled even-odd: fill a
    /// gradient through it for a rim that changes along the edge.
    pub fn rounded_ring(rect: Rect, corners: Corners, width: f32) -> Self {
        let mut path = Self::rounded(rect, corners);
        let width = width.clamp(0.0, rect.width.min(rect.height) / 2.0);
        let less = |r: f32| (r - width).max(0.0);
        let inner = Corners {
            top_left: less(corners.top_left),
            top_right: less(corners.top_right),
            bottom_right: less(corners.bottom_right),
            bottom_left: less(corners.bottom_left),
            smoothing: corners.smoothing,
        };
        path.push_rounded(rect.inset(width), inner);
        path.fill_rule = FillRule::EvenOdd;
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(path: &Path) -> Vec<Point> {
        path.commands
            .iter()
            .flat_map(|c| match *c {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => vec![p],
                PathCommand::QuadTo(a, p) => vec![a, p],
                PathCommand::CubicTo(a, b, p) => vec![a, b, p],
                PathCommand::Close => vec![],
            })
            .collect()
    }

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    #[test]
    fn no_smoothing_is_the_circular_rounded_rect() {
        let rect = Rect::new(10.0, 20.0, 100.0, 60.0);
        let ours = points(&Path::rounded(rect, Corners::all(12.0)));
        let circle = points(&Path::rounded_rect(rect, 12.0));
        // ours has a zero length lead in cubic per corner; the corner ends must match
        for p in &circle {
            assert!(ours.iter().any(|q| close(*p, *q)), "{p:?} missing");
        }
    }

    #[test]
    fn smoothing_starts_the_curve_further_along_the_edge_and_stays_inside() {
        let rect = Rect::new(0.0, 0.0, 200.0, 100.0);
        let path = Path::rounded(rect, Corners::all(20.0).smooth(0.6));
        let PathCommand::MoveTo(start) = path.commands[0] else {
            panic!("starts with a move");
        };
        assert!(close(start, Point::new(200.0 - 32.0, 0.0)), "{start:?}");
        for p in points(&path) {
            assert!(
                p.x >= -1e-3 && p.x <= 200.001 && p.y >= -1e-3 && p.y <= 100.001,
                "{p:?}"
            );
        }
    }

    #[test]
    fn each_corner_takes_its_own_radius_and_a_square_one_stays_square() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let path = Path::rounded(rect, Corners::new(0.0, 30.0, 0.0, 10.0));
        let all = points(&path);
        assert!(
            all.iter().any(|p| close(*p, Point::new(0.0, 0.0))),
            "square top left"
        );
        assert!(
            all.iter().any(|p| close(*p, Point::new(100.0, 100.0))),
            "square bottom right"
        );
        assert!(
            all.iter().any(|p| close(*p, Point::new(70.0, 0.0))),
            "top right curve starts 30 in"
        );
    }

    #[test]
    fn a_ring_is_two_outlines_filled_even_odd() {
        let rect = Rect::new(0.0, 0.0, 100.0, 40.0);
        let ring = Path::rounded_ring(rect, Corners::all(20.0), 2.0);
        assert_eq!(ring.fill_rule, FillRule::EvenOdd);
        let moves = ring
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::MoveTo(_)))
            .count();
        assert_eq!(moves, 2);
        assert_eq!(ring.bounds(), Some(rect));
    }

    #[test]
    fn too_much_radius_gives_up_smoothing_then_radius() {
        let rect = Rect::new(0.0, 0.0, 40.0, 40.0);
        let path = Path::rounded(rect, Corners::all(100.0).smooth(0.6));
        for p in points(&path) {
            assert!(
                p.x >= -1e-3 && p.x <= 40.001 && p.y >= -1e-3 && p.y <= 40.001,
                "{p:?}"
            );
        }
        let PathCommand::MoveTo(start) = path.commands[0] else {
            panic!("starts with a move");
        };
        assert!(
            close(start, Point::new(20.0, 0.0)),
            "a full capsule end: {start:?}"
        );
    }
}
