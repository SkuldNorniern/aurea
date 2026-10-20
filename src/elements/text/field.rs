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

    /// Create a text field with initial content.
    pub fn with_content(content: &str) -> AureaResult<Self> {
        let field = Self::new()?;
        field.set_text(content)?;
        Ok(field)
    }
}

#[cfg(test)]
mod text_field_tests {
    use crate::{AureaResult, Window};

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn a_text_field_starts_empty() -> AureaResult<()> {
        use crate::elements::TextField;

        let _window = Window::new("text field", 200, 100)?;

        let field = TextField::new()?;
        assert_eq!(field.get_content()?, "");
        Ok(())
    }
}
