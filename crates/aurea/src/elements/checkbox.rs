use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

pub struct Checkbox {
    handle: *mut c_void,
}

impl Checkbox {
    pub fn new(label: &str) -> AureaResult<Self> {
        let label = CString::new(label).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_checkbox(label.as_ptr()) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self { handle })
    }

    pub fn set_checked(&mut self, checked: bool) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_checkbox_set_checked(self.handle, if checked { 1 } else { 0 }) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    pub fn get_checked(&self) -> bool {
        unsafe { ng_platform_checkbox_get_checked(self.handle) != 0 }
    }

    pub fn set_enabled(&mut self, enabled: bool) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_checkbox_set_enabled(self.handle, if enabled { 1 } else { 0 }) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }
}

impl Element for Checkbox {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_checkbox_invalidate(self.handle);
        }
    }
}
