use super::native::NativeElement;
use super::traits::{Container, Element};
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::any::Any;
use std::boxed::Box as StdBox;
use std::os::raw::c_void;

/// Layout orientation for a split view divider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOrientation {
    /// Panes arranged horizontally.
    Horizontal,
    /// Panes arranged vertically.
    Vertical,
}

/// A native split view container with a draggable divider.
pub struct SplitView {
    handle: NativeElement,
    _orientation: SplitOrientation,
    _children: Vec<StdBox<dyn Any>>,
}

impl SplitView {
    /// Create a new split view with the given orientation.
    pub fn new(orientation: SplitOrientation) -> AureaResult<Self> {
        let is_vertical = match orientation {
            SplitOrientation::Vertical => 1,
            SplitOrientation::Horizontal => 0,
        };

        let handle = unsafe { ng_platform_create_split_view(is_vertical) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self {
            handle: NativeElement::new(handle),
            _orientation: orientation,
            _children: Vec::new(),
        })
    }

    /// Moves divider `index` (only 0) to `position` logical pixels from the leading edge.
    pub fn set_divider_position(&self, index: i32, position: f32) -> AureaResult<()> {
        if index != 0 || !(0.0..1e7).contains(&position) {
            return Err(AureaError::ElementOperationFailed);
        }
        let result = unsafe {
            ng_platform_split_view_set_divider_position(self.handle.handle(), index, position)
        };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// Create a split view and add two children.
    pub fn with_children<L: Element + 'static, R: Element + 'static>(
        orientation: SplitOrientation,
        left: L,
        right: R,
    ) -> AureaResult<Self> {
        let mut split = Self::new(orientation)?;
        split.add(left)?;
        split.add(right)?;
        Ok(split)
    }
}

impl Element for SplitView {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {}
}

impl Container for SplitView {
    fn add_weighted<E: Element + 'static>(&mut self, element: E, _weight: f32) -> AureaResult<()> {
        let result = unsafe { ng_platform_split_view_add(self.handle.handle(), element.handle()) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        element.released_to_parent();

        self._children.push(StdBox::new(element));
        Ok(())
    }
}

#[cfg(test)]
mod split_view_tests {
    use crate::{AureaResult, Container, Window};

    #[test]
    #[ignore = "creates native elements; run with --ignored"]
    fn a_split_view_takes_ownership_of_what_it_is_given() -> AureaResult<()> {
        use crate::elements::{Label, SplitOrientation, SplitView};

        let _window = Window::new("split ownership", 300, 200)?;

        let mut split = SplitView::new(SplitOrientation::Horizontal)?;
        split.add(Label::new("left")?)?;
        split.add(Label::new("right")?)?;
        assert!(
            split.add(Label::new("third")?).is_err(),
            "two panes, as on every platform"
        );
        split.set_divider_position(0, 120.0)?;
        assert!(split.set_divider_position(1, 120.0).is_err());
        assert!(split.set_divider_position(0, -1.0).is_err());
        assert!(split.set_divider_position(0, f32::NAN).is_err());
        assert!(split.set_divider_position(0, f32::INFINITY).is_err());
        drop(split);
        Ok(())
    }
}
