//! Pixel-space scissor rectangle for the CPU rasterizer.
//!
//! Every draw routine used to clamp its span and row ranges against the
//! framebuffer's `0..width` / `0..height`. A [`ClipBox`] replaces those raw
//! bounds so the same clamping also enforces the item's active clip: the
//! rasterizer never has to know *why* a region is off limits.
//!
//! Clips are resolved when the display list is recorded, not replayed as a
//! push/pop command stream — partial repaint renders an arbitrary *subset* of
//! items, so any state that only exists between a push and a pop would be
//! missing exactly when the frame is repainted in pieces.

use crate::numeric::f32_to_u32_clamped;
use crate::types::Rect;

/// A half-open pixel rectangle, `x0..x1` by `y0..y1`, in physical pixels.
///
/// Always within the framebuffer: constructed from the surface size and only
/// ever narrowed from there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipBox {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl ClipBox {
    /// The whole framebuffer — the widest a clip can be.
    pub fn surface(width: u32, height: u32) -> Self {
        Self {
            x0: 0,
            y0: 0,
            x1: width,
            y1: height,
        }
    }

    /// A box that clips nothing, for callers that do their own bounds checks.
    pub fn unbounded() -> Self {
        Self::surface(u32::MAX, u32::MAX)
    }

    /// Narrows to `rect` (physical pixels), snapping to the pixel grid.
    ///
    /// Edges round to the nearest pixel boundary rather than expanding
    /// outward: a clip that lets a half-covered pixel through would defeat the
    /// point of clipping.
    pub fn intersect(self, rect: Rect) -> Self {
        let snap = |v: f32| f32_to_u32_clamped((v + 0.5).floor().max(0.0));
        Self {
            x0: self.x0.max(snap(rect.x)),
            y0: self.y0.max(snap(rect.y)),
            x1: self.x1.min(snap(rect.x + rect.width)),
            y1: self.y1.min(snap(rect.y + rect.height)),
        }
    }

    /// Narrows to `rect` if there is one.
    pub fn intersect_opt(self, rect: Option<Rect>) -> Self {
        match rect {
            Some(rect) => self.intersect(rect),
            None => self,
        }
    }

    /// Whether the box admits no pixels at all.
    pub fn is_empty(self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    /// Whether a pixel coordinate is inside the box.
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= 0
            && y >= 0
            && x.unsigned_abs() >= self.x0
            && x.unsigned_abs() < self.x1
            && y.unsigned_abs() >= self.y0
            && y.unsigned_abs() < self.y1
    }

    /// Left edge as a float, for clamping geometry before it is rounded.
    pub fn left(self) -> f32 {
        self.x0 as f32
    }

    /// Right edge as a float.
    pub fn right(self) -> f32 {
        self.x1 as f32
    }

    /// Top edge as a float.
    pub fn top(self) -> f32 {
        self.y0 as f32
    }

    /// Bottom edge as a float.
    pub fn bottom(self) -> f32 {
        self.y1 as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_covers_everything() {
        let c = ClipBox::surface(10, 20);
        assert!(c.contains(0, 0));
        assert!(c.contains(9, 19));
        assert!(!c.contains(10, 0));
        assert!(!c.contains(-1, 0));
    }

    #[test]
    fn intersect_narrows_and_never_widens() {
        let c = ClipBox::surface(10, 10).intersect(Rect::new(2.0, 3.0, 4.0, 4.0));
        assert_eq!((c.x0, c.y0, c.x1, c.y1), (2, 3, 6, 7));

        let outside = ClipBox::surface(10, 10).intersect(Rect::new(-5.0, -5.0, 100.0, 100.0));
        assert_eq!(outside, ClipBox::surface(10, 10));
    }

    #[test]
    fn disjoint_intersection_is_empty() {
        let c = ClipBox::surface(10, 10).intersect(Rect::new(20.0, 20.0, 5.0, 5.0));
        assert!(c.is_empty());
    }

    #[test]
    fn nested_intersections_compose() {
        let c = ClipBox::surface(20, 20)
            .intersect(Rect::new(2.0, 2.0, 10.0, 10.0))
            .intersect(Rect::new(6.0, 0.0, 10.0, 5.0));
        assert_eq!((c.x0, c.y0, c.x1, c.y1), (6, 2, 12, 5));
    }
}
