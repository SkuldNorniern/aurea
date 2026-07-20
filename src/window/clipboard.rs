use crate::ffi::{
    ng_platform_free_clipboard_text, ng_platform_get_clipboard_text, ng_platform_set_clipboard_text,
};
use crate::{AureaError, AureaResult};
use std::ffi::{CStr, CString};

/// Read the OS clipboard as a UTF-8 string.
/// Returns `None` if the clipboard is empty or does not contain text.
pub fn clipboard_text() -> Option<String> {
    let ptr = unsafe { ng_platform_get_clipboard_text() };
    if ptr.is_null() {
        return None;
    }
    let text = unsafe {
        let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        ng_platform_free_clipboard_text(ptr);
        s
    };
    if text.is_empty() { None } else { Some(text) }
}

/// Write a UTF-8 string to the OS clipboard.
pub fn set_clipboard_text(text: &str) -> AureaResult<()> {
    let Ok(cstr) = CString::new(text) else {
        return Err(AureaError::ElementOperationFailed);
    };
    let result = unsafe { ng_platform_set_clipboard_text(cstr.as_ptr()) };
    if result != 0 {
        Err(AureaError::ElementOperationFailed)
    } else {
        Ok(())
    }
}
