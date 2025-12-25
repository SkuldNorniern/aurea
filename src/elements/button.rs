use std::{ffi::CString, os::raw::c_void};
use crate::{AureaError, AureaResult, ffi::*};
use super::traits::Element;

pub struct Button {
    handle: *mut c_void,
    _title: CString,
}

impl Button {
    pub fn new(title: &str) -> AureaResult<Self> {
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_button(title.as_ptr()) };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(Self {
            handle,
            _title: title,
        })
    }
}

impl Element for Button {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
}

