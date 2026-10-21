//! Single-line text input field.

use super::base::TextBase;
use super::{TextControl, TextFeatures, text_widget};
use crate::AureaResult;
use crate::ffi::ng_platform_create_text_field;

/// One line of editable text: a native EDIT, GtkEntry or NSTextField.
pub struct TextField {
    base: TextBase,
}

text_widget!(TextField);

impl TextField {
    /// Creates an empty text field with the default field features.
    pub fn new() -> AureaResult<Self> {
        Self::with_features(TextFeatures::FIELD)
    }

    /// Creates a text field with the given features.
    pub fn with_features(features: TextFeatures) -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_text_field() };
        Ok(Self {
            base: TextBase::new(handle, false, features)?,
        })
    }

    /// Shows dots in place of the text, for passwords. The text still reads back as typed; the
    /// platform blocks copying it out.
    pub fn set_password(&self, on: bool) -> AureaResult<()> {
        self.base.set_secret(on)
    }

    /// Create a text field with initial content.
    pub fn with_content(content: &str) -> AureaResult<Self> {
        let field = Self::new()?;
        field.set_text(content)?;
        Ok(field)
    }
}

#[cfg(test)]
mod text_field_tests {
    use crate::ffi::{NG_TEXT_FEATURE_SECRET, ng_platform_text_set_feature};
    use crate::{AureaResult, Window};
    use std::ffi::c_void;

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn a_text_field_starts_empty() -> AureaResult<()> {
        use crate::elements::TextField;

        let _window = Window::new("text field", 200, 100)?;

        let field = TextField::new()?;
        assert_eq!(field.get_content()?, "");
        Ok(())
    }

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn a_password_field_hides_its_text_but_keeps_it() -> AureaResult<()> {
        use crate::elements::{Element, TextField, TextView};

        let _window = Window::new("password", 200, 100)?;
        let field = TextField::with_content("hunter2")?;
        field.set_password(true)?;
        assert!(hidden(field.handle()));
        assert_eq!(field.get_content()?, "hunter2");
        field.set_password(false)?;
        assert!(!hidden(field.handle()));
        let view = TextView::new(true)?;
        let refused =
            unsafe { ng_platform_text_set_feature(view.handle(), NG_TEXT_FEATURE_SECRET, 1) };
        assert_ne!(refused, 0, "only single-line fields take a password mode");
        Ok(())
    }

    #[cfg(windows)]
    fn hidden(handle: *mut c_void) -> bool {
        unsafe extern "system" {
            fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
        }
        const EM_GETPASSWORDCHAR: u32 = 0x00D2;
        unsafe { SendMessageW(handle, EM_GETPASSWORDCHAR, 0, 0) != 0 }
    }

    #[cfg(target_os = "linux")]
    fn hidden(handle: *mut c_void) -> bool {
        unsafe extern "C" {
            fn gtk_entry_get_visibility(entry: *mut c_void) -> i32;
        }
        unsafe { gtk_entry_get_visibility(handle) == 0 }
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn hidden(_handle: *mut c_void) -> bool {
        true
    }
}
