use std::{ffi::CString, os::raw::c_void};
use crate::{AureaError, AureaResult, ffi::*};
use super::traits::Element;

pub struct TextEditor {
    handle: *mut c_void,
}

impl TextEditor {
    pub fn new() -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_text_editor() };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(Self { handle })
    }

    pub fn set_content(&mut self, content: &str) -> AureaResult<()> {
        let content = CString::new(content).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe {
            ng_platform_set_text_content(self.handle, content.as_ptr())
        };
        
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(())
    }

    pub fn get_content(&self) -> AureaResult<String> {
        let content_ptr = unsafe { ng_platform_get_text_content(self.handle) };
        
        if content_ptr.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        let content = unsafe {
            let cstr = std::ffi::CStr::from_ptr(content_ptr);
            let result = cstr.to_str()
                .map_err(|_| AureaError::ElementOperationFailed)?
                .to_string();
            ng_platform_free_text_content(content_ptr);
            result
        };
        
        Ok(content)
    }
}

impl Element for TextEditor {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
}

