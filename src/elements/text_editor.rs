use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

pub struct TextEditor {
    handle: *mut c_void,
    _id: u32,
}

impl TextEditor {
    pub fn new() -> AureaResult<Self> {
        Self::with_callback(|_| {})
    }

    pub fn with_callback<F>(callback: F) -> AureaResult<Self>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        let id = crate::registry::elements::next_text_editor_id();

        let handle = unsafe { ng_platform_create_text_editor(id) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        crate::registry::elements::register_text_editor_callback(id, callback);

        Ok(Self { handle, _id: id })
    }

    pub fn set_content(&mut self, content: &str) -> AureaResult<()> {
        let content = CString::new(content).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_set_text_content(self.handle, content.as_ptr()) };

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
            let result = cstr
                .to_str()
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

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_text_editor_invalidate(self.handle);
        }
    }
}

pub(crate) fn invoke_text_callback(id: u32, content: String) {
    crate::registry::elements::invoke_text_editor_callback(id, content);
}
