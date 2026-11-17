//! Single-line text input field.

use super::base::TextBase;
use super::{TextControl, TextFeatures, text_widget};
use crate::AureaResult;
use crate::elements::Element;
use crate::ffi::{
    c_text, ng_platform_create_search_field, ng_platform_create_text_field,
    ng_platform_text_set_placeholder, status,
};

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

    /// A field for searching: the platform's search box on macOS, Linux and iOS, with its mark and
    /// clear button; a plain field on Windows.
    pub fn search() -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_search_field() };
        Ok(Self {
            base: TextBase::new(handle, false, TextFeatures::FIELD)?,
        })
    }

    /// What shows while the field is empty.
    pub fn set_placeholder(&self, text: &str) -> AureaResult<()> {
        let text = c_text(text)?;
        status("TextField::set_placeholder", unsafe {
            ng_platform_text_set_placeholder(self.handle(), text.as_ptr())
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

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;
    use crate::Window;

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn a_search_field_is_a_text_field() -> AureaResult<()> {
        let _window = Window::new("test", 200, 100)?;
        let mut field = TextField::search()?;
        field.set_placeholder("Find")?;
        field.set_content("layer")?;
        assert_eq!(field.get_content()?, "layer");
        Ok(())
    }

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn a_tooltip_sets_and_clears() -> AureaResult<()> {
        let _window = Window::new("test", 200, 100)?;
        let field = TextField::new()?;
        field.set_tooltip("What goes here")?;
        field.set_tooltip("Something else")?;
        field.set_tooltip("")?;
        Ok(())
    }
}
