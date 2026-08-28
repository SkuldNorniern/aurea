//! What needs repainting.
//!
//! Only the union is kept. The individual rectangles used to be collected too,
//! up to a cap, but nothing ever read them: `take` returned the union and the
//! list was thrown away. Storing them cost an allocation per canvas and implied
//! a multi-region repaint that does not exist.
//!
//! The renderer's tile grid is what actually gives fine-grained repaint, and it
//! works from the display-list diff rather than from this. If real multi-region
//! damage is ever wanted, it belongs here — but with something reading it.

use aurea_foundation::Rect;

/// The area to repaint, accumulated as one rectangle.
#[derive(Debug, Clone, Copy, Default)]
pub struct DamageRegion {
    union: Option<Rect>,
}

impl DamageRegion {
    /// A region with nothing damaged.
    ///
    /// `capacity` is ignored; it stays in the signature so existing callers do
    /// not have to change, and because a future multi-region implementation
    /// would want it back.
    pub fn new(_capacity: usize) -> Self {
        Self { union: None }
    }

    /// Grows the damaged area to include `rect`.
    ///
    /// A rectangle with no area is ignored rather than dragging the union out
    /// to wherever it sits.
    pub fn add(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        self.union = Some(match self.union {
            Some(current) => current.union(rect),
            None => rect,
        });
    }

    /// Marks everything damaged without naming an area.
    ///
    /// The caller does not always know the size, and `None` already means "no
    /// hint" to the renderer, which then repaints the lot.
    pub fn add_all(&mut self) {
        self.union = None;
    }

    /// Marks a whole surface of this size damaged.
    pub fn set_full(&mut self, width: f32, height: f32) {
        self.union = Some(Rect::new(0.0, 0.0, width, height));
    }

    /// Takes the damaged area and clears it.
    pub fn take(&mut self) -> Option<Rect> {
        self.union.take()
    }

    /// Clears without reading.
    pub fn clear(&mut self) {
        self.union = None;
    }

    /// Whether any area is damaged.
    pub fn is_empty(&self) -> bool {
        self.union.is_none()
    }

    /// The damaged area, without clearing it.
    pub fn union(&self) -> Option<Rect> {
        self.union
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_region_is_empty() {
        let region = DamageRegion::new(16);
        assert!(region.is_empty());
        assert_eq!(region.union(), None);
    }

    #[test]
    fn adding_grows_the_area() {
        let mut region = DamageRegion::new(16);
        region.add(Rect::new(0.0, 0.0, 10.0, 10.0));
        region.add(Rect::new(20.0, 20.0, 5.0, 5.0));

        assert_eq!(region.union(), Some(Rect::new(0.0, 0.0, 25.0, 25.0)));
    }

    /// An empty rect at the origin would otherwise pull the union back to the
    /// top-left corner and repaint the whole screen.
    #[test]
    fn an_empty_rect_is_ignored() {
        let mut region = DamageRegion::new(16);
        region.add(Rect::new(50.0, 50.0, 10.0, 10.0));
        region.add(Rect::new(0.0, 0.0, 0.0, 0.0));

        assert_eq!(region.union(), Some(Rect::new(50.0, 50.0, 10.0, 10.0)));
    }

    #[test]
    fn take_clears_the_region() {
        let mut region = DamageRegion::new(16);
        region.add(Rect::new(1.0, 2.0, 3.0, 4.0));

        assert_eq!(region.take(), Some(Rect::new(1.0, 2.0, 3.0, 4.0)));
        assert!(region.is_empty());
        assert_eq!(region.take(), None);
    }

    #[test]
    fn add_all_means_no_hint() {
        let mut region = DamageRegion::new(16);
        region.add(Rect::new(1.0, 2.0, 3.0, 4.0));
        region.add_all();

        // `None` tells the renderer to repaint everything.
        assert_eq!(region.take(), None);
    }

    #[test]
    fn set_full_names_the_whole_surface() {
        let mut region = DamageRegion::new(16);
        region.set_full(800.0, 600.0);

        assert_eq!(region.union(), Some(Rect::new(0.0, 0.0, 800.0, 600.0)));
    }

    #[test]
    fn clear_forgets_without_reading() {
        let mut region = DamageRegion::new(16);
        region.add(Rect::new(1.0, 1.0, 1.0, 1.0));
        region.clear();

        assert!(region.is_empty());
    }

    #[test]
    fn many_adds_do_not_grow_anything() {
        let mut region = DamageRegion::new(4);
        for i in 0..1000u16 {
            let f = f32::from(i);
            region.add(Rect::new(f, f, 1.0, 1.0));
        }

        // The cap used to decide how many rects were kept; there is one now.
        assert_eq!(size_of::<DamageRegion>(), size_of::<Option<Rect>>());
        assert!(region.union().is_some());
    }
}
