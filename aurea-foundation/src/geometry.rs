//! Points and rectangles.
//!
//! These live in the foundation rather than in the renderer because they are
//! not a rendering idea. The runtime describes damage with a rectangle, layout
//! and hit testing use them too, and none of that should have to depend on a
//! renderer to say where something is. `aurea-render` re-exports them, so a
//! caller sees one `Rect` wherever it comes from.

/// A position, in whatever space the caller is working in.
///
/// The type does not say whether it is logical or physical; that is the
/// caller's business, and mixing the two is a bug the type cannot catch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Distance to another point.
    pub fn distance_to(self, other: Self) -> f32 {
        (other.x - self.x).hypot(other.y - self.y)
    }
}

/// A rectangle, given by its top-left corner and its size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// A rectangle spanning two corners.
    ///
    /// The corners are taken as given: passing them the wrong way round gives
    /// a negative size rather than a silently corrected rectangle.
    pub fn from_points(top_left: Point, bottom_right: Point) -> Self {
        Self {
            x: top_left.x,
            y: top_left.y,
            width: bottom_right.x - top_left.x,
            height: bottom_right.y - top_left.y,
        }
    }

    /// The x just past the right edge.
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    /// The y just past the bottom edge.
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    /// Whether the rectangle covers no area.
    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    /// Whether `point` falls inside, edges included.
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.right()
            && point.y >= self.y
            && point.y <= self.bottom()
    }

    /// Whether the two rectangles share any area. Touching edges do not count.
    pub fn intersects(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// The area the two have in common, or `None` when they have none.
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= x || bottom <= y {
            return None;
        }
        Some(Self::new(x, y, right - x, bottom - y))
    }

    /// The smallest rectangle holding both.
    ///
    /// An empty rectangle contributes nothing, so a union with one is the
    /// other. Otherwise a zero-sized rect at the origin would drag the result
    /// back to the corner of the screen.
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(x, y, right - x, bottom - y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rect_knows_its_far_edges() {
        let r = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.right(), 40.0);
        assert_eq!(r.bottom(), 60.0);
    }

    #[test]
    fn from_points_keeps_the_corners_as_given() {
        let r = Rect::from_points(Point::new(1.0, 2.0), Point::new(5.0, 8.0));
        assert_eq!(r, Rect::new(1.0, 2.0, 4.0, 6.0));
    }

    #[test]
    fn contains_includes_the_edges() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains(Point::new(0.0, 0.0)));
        assert!(r.contains(Point::new(10.0, 10.0)));
        assert!(!r.contains(Point::new(10.1, 5.0)));
    }

    #[test]
    fn touching_rects_do_not_intersect() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(10.0, 0.0, 10.0, 10.0);
        assert!(!a.intersects(b));
        assert_eq!(a.intersection(b), None);
    }

    #[test]
    fn overlapping_rects_share_an_area() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert!(a.intersects(b));
        assert_eq!(a.intersection(b), Some(Rect::new(5.0, 5.0, 5.0, 5.0)));
    }

    #[test]
    fn union_covers_both() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(20.0, 20.0, 5.0, 5.0);
        assert_eq!(a.union(b), Rect::new(0.0, 0.0, 25.0, 25.0));
    }

    /// A zero-sized rect at the origin would otherwise drag the union back to
    /// the top-left corner of the screen.
    #[test]
    fn union_with_an_empty_rect_is_the_other_one() {
        let a = Rect::new(50.0, 50.0, 10.0, 10.0);
        let empty = Rect::new(0.0, 0.0, 0.0, 0.0);
        assert_eq!(a.union(empty), a);
        assert_eq!(empty.union(a), a);
    }

    #[test]
    fn an_empty_rect_knows_it() {
        assert!(Rect::new(0.0, 0.0, 0.0, 10.0).is_empty());
        assert!(Rect::new(0.0, 0.0, 10.0, -1.0).is_empty());
        assert!(!Rect::new(0.0, 0.0, 1.0, 1.0).is_empty());
    }

    #[test]
    fn distance_between_points() {
        assert_eq!(Point::new(0.0, 0.0).distance_to(Point::new(3.0, 4.0)), 5.0);
    }
}
