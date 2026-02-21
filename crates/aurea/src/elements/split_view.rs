use super::traits::{Container, Element};
use crate::{AureaError, AureaResult, ffi::*};
use std::os::raw::c_void;

/// Layout orientation for a split view divider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOrientation {
    Horizontal,
    Vertical,
}

/// A native split view container with a draggable divider.
pub struct SplitView {
    handle: *mut c_void,
    _orientation: SplitOrientation,
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
            handle,
            _orientation: orientation,
        })
    }

    /// Set the divider position for the given split index.
    pub fn set_divider_position(&self, index: i32, position: f32) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_split_view_set_divider_position(self.handle, index, position) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// Create a split view and add two children.
    pub fn with_children<L: Element, R: Element>(
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
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        // NSSplitView handles its own invalidation usually,
        // but we could call a specific platform function if needed.
    }
}

impl Container for SplitView {
    fn add_weighted<E: Element>(&mut self, element: E, _weight: f32) -> AureaResult<()> {
        // Note: NSSplitView doesn't use simple weights like NSStackView,
        // it uses constraints or holding priorities. For now, we just add it.
        let result = unsafe { ng_platform_split_view_add(self.handle, element.handle()) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }
}
