use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::os::raw::c_void;

pub struct Slider {
    handle: *mut c_void,
}

impl Slider {
    pub fn new(min: f64, max: f64) -> AureaResult<Self> {
        if min >= max {
            return Err(AureaError::ElementOperationFailed);
        }

        let handle = unsafe { ng_platform_create_slider(min, max) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self { handle })
    }

    pub fn set_value(&mut self, value: f64) -> AureaResult<()> {
        let result = unsafe { ng_platform_slider_set_value(self.handle, value) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    pub fn get_value(&self) -> f64 {
        unsafe { ng_platform_slider_get_value(self.handle) }
    }

    pub fn set_enabled(&mut self, enabled: bool) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_slider_set_enabled(self.handle, if enabled { 1 } else { 0 }) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }
}

impl Element for Slider {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_slider_invalidate(self.handle);
        }
    }
}
