use super::traits::{Container, Element};
use crate::{ffi::*, AureaError, AureaResult};
use std::os::raw::c_void;

#[derive(Debug, Clone, Copy)]
pub enum BoxOrientation {
    Horizontal,
    Vertical,
}

pub struct Box {
    handle: *mut c_void,
    _orientation: BoxOrientation,
}

impl Box {
    pub fn new(orientation: BoxOrientation) -> AureaResult<Self> {
        let is_vertical = match orientation {
            BoxOrientation::Vertical => 1,
            BoxOrientation::Horizontal => 0,
        };

        let handle = unsafe { ng_platform_create_box(is_vertical) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self {
            handle,
            _orientation: orientation,
        })
    }
}

impl Element for Box {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_box_invalidate(self.handle);
        }
    }
}

impl Container for Box {
    fn add_weighted<E: Element>(&mut self, element: E, weight: f32) -> AureaResult<()> {
        let result = unsafe { ng_platform_box_add(self.handle, element.handle(), weight) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }
}
