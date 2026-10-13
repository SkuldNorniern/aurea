use std::f32::consts::{FRAC_PI_2, TAU};

use super::{Point, Rect};
use crate::vector::path_data::Arc;

/// Path command for drawing paths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    /// Move to a point.
    MoveTo(Point),
    /// Line to a point.
    LineTo(Point),
    /// Quadratic curve to a point.
    QuadTo(Point, Point),
    /// Cubic curve to a point.
    CubicTo(Point, Point, Point),
    /// Close the path.
    Close,
}

/// Which points a path's outline encloses when its contours overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FillRule {
    /// Inside where the contours wind around a point a nonzero number of times: a contour wound the
    /// other way cuts a hole.
    #[default]
    NonZero,
    /// Inside where an odd number of contours cross on the way out.
    EvenOdd,
}

/// Path for drawing shapes.
#[derive(Debug, Clone)]
pub struct Path {
    pub commands: Vec<PathCommand>,
    pub fill_rule: FillRule,
}

impl Path {
    /// Where the next command starts from: the last point, or the start of the shape a `Close` just
    /// closed.
    pub fn current_point(&self) -> Option<Point> {
        let mut start = None;
        let mut at = None;
        for command in &self.commands {
            match *command {
                PathCommand::MoveTo(p) => {
                    start = Some(p);
                    at = Some(p);
                }
                PathCommand::LineTo(p)
                | PathCommand::QuadTo(_, p)
                | PathCommand::CubicTo(_, _, p) => {
                    at = Some(p);
                }
                PathCommand::Close => at = start,
            }
        }
        at
    }

    /// An elliptical arc to `to`, as SVG's `A` command: `radii`, the ellipse turned by `rotation`
    /// radians, and which of the four arcs.
    pub fn arc_to(
        &mut self,
        radii: (f32, f32),
        rotation: f32,
        (large, sweep): (bool, bool),
        to: Point,
    ) {
        let from = self.current_point().unwrap_or_else(|| {
            self.commands
                .push(PathCommand::MoveTo(Point::new(0.0, 0.0)));
            Point::new(0.0, 0.0)
        });
        let arc = Arc {
            radii,
            rotation,
            large,
            sweep,
        };
        for (c1, c2, end) in arc.cubics(from, to) {
            self.commands.push(PathCommand::CubicTo(c1, c2, end));
        }
    }

    /// A circular arc around `center` from angle `start` to `end` (radians, clockwise on screen, as
    /// y grows down), going the other way when `counter_clockwise`.
    pub fn arc(
        &mut self,
        center: Point,
        radius: f32,
        (start, end): (f32, f32),
        counter_clockwise: bool,
    ) {
        let point = |angle: f32| {
            Point::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        };
        let first = point(start);
        match self.current_point() {
            None => self.commands.push(PathCommand::MoveTo(first)),
            Some(at) if (at.x - first.x).abs() > 1e-4 || (at.y - first.y).abs() > 1e-4 => {
                self.commands.push(PathCommand::LineTo(first));
            }
            Some(_) => {}
        }
        let sweep = arc_sweep(start, end, counter_clockwise);
        let segments = (sweep.abs() / FRAC_PI_2).ceil().max(1.0);
        let step = sweep / segments;
        let k = 4.0 / 3.0 * (step / 4.0).tan() * radius;
        let mut angle = start;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for _ in 0..segments as u32 {
            let next = angle + step;
            let (a, b) = (point(angle), point(next));
            let c1 = Point::new(a.x - k * angle.sin(), a.y + k * angle.cos());
            let c2 = Point::new(b.x + k * next.sin(), b.y - k * next.cos());
            self.commands.push(PathCommand::CubicTo(c1, c2, b));
            angle = next;
        }
    }

    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            fill_rule: FillRule::NonZero,
        }
    }

    /// The outline of `rect` with corners of `radius`, kept to half the shorter side, as four cubic
    /// arcs.
    pub fn rounded_rect(rect: Rect, radius: f32) -> Self {
        let mut path = Self {
            commands: Vec::with_capacity(10),
            fill_rule: FillRule::NonZero,
        };
        path.push_rounded_rect(rect, radius);
        path
    }

    /// Adds [`Path::rounded_rect`]'s outline to this path, so one path can be refilled every frame
    /// without allocating again.
    pub fn push_rounded_rect(&mut self, rect: Rect, radius: f32) {
        let r = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
        let k = r * 0.552_284_8;
        let (x0, y0) = (rect.x, rect.y);
        let (x1, y1) = (rect.x + rect.width, rect.y + rect.height);
        let p = Point::new;
        self.commands.extend_from_slice(&[
            PathCommand::MoveTo(p(x0 + r, y0)),
            PathCommand::LineTo(p(x1 - r, y0)),
            PathCommand::CubicTo(p(x1 - r + k, y0), p(x1, y0 + r - k), p(x1, y0 + r)),
            PathCommand::LineTo(p(x1, y1 - r)),
            PathCommand::CubicTo(p(x1, y1 - r + k), p(x1 - r + k, y1), p(x1 - r, y1)),
            PathCommand::LineTo(p(x0 + r, y1)),
            PathCommand::CubicTo(p(x0 + r - k, y1), p(x0, y1 - r + k), p(x0, y1 - r)),
            PathCommand::LineTo(p(x0, y0 + r)),
            PathCommand::CubicTo(p(x0, y0 + r - k), p(x0 + r - k, y0), p(x0 + r, y0)),
            PathCommand::Close,
        ]);
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

fn arc_sweep(start: f32, end: f32, counter_clockwise: bool) -> f32 {
    let raw = end - start;
    if raw.abs() >= TAU {
        return if counter_clockwise { -TAU } else { TAU };
    }
    let turn = raw.rem_euclid(TAU);
    if counter_clockwise {
        if turn == 0.0 { 0.0 } else { turn - TAU }
    } else {
        turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end(path: &Path) -> Point {
        path.current_point().expect("a point")
    }

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    #[test]
    fn a_quarter_circle_ends_where_it_should_and_stays_round() {
        let mut path = Path::new();
        path.arc(Point::new(0.0, 0.0), 10.0, (0.0, FRAC_PI_2), false);
        assert!(close(end(&path), Point::new(0.0, 10.0)), "{:?}", end(&path));
        let PathCommand::CubicTo(c1, c2, p) = path.commands[1] else {
            panic!("a cubic");
        };
        let start = Point::new(10.0, 0.0);
        let mid = Point::new(
            (start.x + 3.0 * c1.x + 3.0 * c2.x + p.x) / 8.0,
            (start.y + 3.0 * c1.y + 3.0 * c2.y + p.y) / 8.0,
        );
        assert!((mid.x.hypot(mid.y) - 10.0).abs() < 0.01, "{mid:?}");
    }

    #[test]
    fn a_counter_clockwise_arc_goes_the_short_way_round() {
        let mut path = Path::new();
        path.arc(Point::new(0.0, 0.0), 10.0, (0.0, FRAC_PI_2), true);
        assert!(close(end(&path), Point::new(0.0, 10.0)));
        assert_eq!(
            path.commands.len(),
            4,
            "three quarters, three cubics after the move"
        );
    }

    #[test]
    fn two_svg_arcs_make_a_closed_ellipse() {
        let mut path = Path::new();
        path.commands
            .push(PathCommand::MoveTo(Point::new(0.0, 0.0)));
        path.arc_to((20.0, 10.0), 0.0, (false, true), Point::new(40.0, 0.0));
        path.arc_to((20.0, 10.0), 0.0, (false, true), Point::new(0.0, 0.0));
        assert!(close(end(&path), Point::new(0.0, 0.0)));
        assert!(path.commands.len() >= 5, "split into cubics");
    }

    #[test]
    fn a_zero_radius_arc_is_a_line() {
        let mut path = Path::new();
        path.commands
            .push(PathCommand::MoveTo(Point::new(0.0, 0.0)));
        path.arc_to((0.0, 5.0), 0.0, (false, false), Point::new(10.0, 0.0));
        assert!(close(end(&path), Point::new(10.0, 0.0)));
    }

    #[test]
    fn the_current_point_follows_a_close_back_to_the_start() {
        let mut path = Path::new();
        assert_eq!(path.current_point(), None);
        path.commands
            .push(PathCommand::MoveTo(Point::new(1.0, 2.0)));
        path.commands
            .push(PathCommand::LineTo(Point::new(5.0, 2.0)));
        path.commands.push(PathCommand::Close);
        assert_eq!(path.current_point(), Some(Point::new(1.0, 2.0)));
    }
}
