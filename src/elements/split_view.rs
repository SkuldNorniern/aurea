use super::traits::{Container, Element};
use crate::{AureaError, AureaResult, ffi::*};
use std::os::raw::c_void;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOrientation {
    Horizontal,
    Vertical,
}

pub struct SplitView {
    handle: *mut c_void,
    _orientation: SplitOrientation,
}

impl SplitView {
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

    pub fn set_divider_position(&self, index: i32, position: f32) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_split_view_set_divider_position(self.handle, index, position) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
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
