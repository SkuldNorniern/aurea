use super::{Point, Rect};

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
