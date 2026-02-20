//! Single-line text input field.
//!
//! Supported on Windows; macOS and Linux return error (not yet implemented).

use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

/// Single-line editable text field.
///
/// Uses native platform control: Windows EDIT, etc.
/// Returns error on macOS and Linux (ng_platform_create_text_field is NULL).
pub struct TextField {
    handle: *mut c_void,
}

impl TextField {
    /// Create a new text field.
    pub fn new() -> AureaResult<Self> {
        Self::with_content("")
    }

    /// Create a text field with initial content.
    pub fn with_content(content: &str) -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_text_field() };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        let mut field = Self { handle };
        let _ = field.set_content(content);
        Ok(field)
    }

    /// Set the text content.
    pub fn set_content(&mut self, content: &str) -> AureaResult<()> {
        let content = CString::new(content).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_set_text_content(self.handle, content.as_ptr()) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// Get the text content.
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

impl Element for TextField {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_text_editor_invalidate(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_field_creation_may_fail_on_unsupported_platform() {
        let result = TextField::new();
        if result.is_ok() {
            let tf = result.unwrap();
            let content = tf.get_content();
            assert!(content.is_ok());
            assert_eq!(content.unwrap(), "");
        }
    }
}
