//! Single-line text input field.
//!
//! Uses a native single-line field where available and falls back to editable TextView
//! on platforms that do not yet expose `ng_platform_create_text_field`.

use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextFieldKind {
    NativeField,
    EditableTextViewFallback,
}

/// Single-line editable text field.
///
/// Uses native platform control: Windows EDIT, etc.
/// Returns error on macOS and Linux (ng_platform_create_text_field is NULL).
pub struct TextField {
    handle: *mut c_void,
    kind: TextFieldKind,
}

impl TextField {
    /// Create a new text field.
    pub fn new() -> AureaResult<Self> {
        Self::with_content("")
    }

    /// Create a text field with initial content.
    pub fn with_content(content: &str) -> AureaResult<Self> {
        let (handle, kind) = unsafe {
            let handle = ng_platform_create_text_field();
            if !handle.is_null() {
                (handle, TextFieldKind::NativeField)
            } else {
                #[cfg(target_os = "macos")]
                {
                    return Err(AureaError::ElementOperationFailed);
                }

                #[cfg(not(target_os = "macos"))]
                {
                    static TEXT_FIELD_FALLBACK_ID: std::sync::LazyLock<std::sync::Mutex<u32>> =
                        std::sync::LazyLock::new(|| std::sync::Mutex::new(1));
                    let fallback_id = {
                        let mut id_guard = crate::sync::lock(&TEXT_FIELD_FALLBACK_ID);
                        *id_guard += 1;
                        *id_guard - 1
                    };
                    let fallback = ng_platform_create_text_view(1, fallback_id);
                    if fallback.is_null() {
                        return Err(AureaError::ElementOperationFailed);
                    }
                    (fallback, TextFieldKind::EditableTextViewFallback)
                }
            }
        };

        let mut field = Self { handle, kind };
        let _ = field.set_content(content);
        Ok(field)
    }

    /// Returns true when this field is backed by an editable TextView fallback.
    pub fn is_fallback(&self) -> bool {
        self.kind == TextFieldKind::EditableTextViewFallback
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
            match self.kind {
                TextFieldKind::NativeField => ng_platform_text_editor_invalidate(self.handle),
                TextFieldKind::EditableTextViewFallback => {
                    ng_platform_text_view_invalidate(self.handle)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_field_creation_is_platform_specific() {
        let result = TextField::new();
        #[cfg(target_os = "windows")]
        {
            assert!(result.is_ok());
            let tf = result.unwrap();
            let content = tf.get_content();
            assert!(content.is_ok());
            assert_eq!(content.unwrap(), "");
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = result;
        }
    }
}
