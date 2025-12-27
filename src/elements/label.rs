use std::{ffi::CString, os::raw::c_void};
use crate::{AureaError, AureaResult, ffi::*};
use super::traits::Element;

pub struct Label {
    handle: *mut c_void,
    _text: CString,
}

impl Label {
    pub fn new(text: &str) -> AureaResult<Self> {
        let text = CString::new(text).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_label(text.as_ptr()) };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(Self {
            handle,
            _text: text,
        })
    }
}

impl Element for Label {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
    
    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        ng_platform_label_invalidate(self.handle);
    }
}

