//! Multi-line text for source.

use super::base::TextBase;
use super::{TextControl, TextFeatures, text_widget};
use crate::AureaResult;
use crate::ffi::ng_platform_create_text_editor;

/// Source text: a fixed-width face, lines running on, undo.
pub struct TextEditor {
    base: TextBase,
}

text_widget!(TextEditor);

impl TextEditor {
    /// Creates a text editor with the default editor features.
    pub fn new() -> AureaResult<Self> {
        Self::with_features(TextFeatures::EDITOR)
    }

    /// Creates a text editor with the given features.
    pub fn with_features(features: TextFeatures) -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_text_editor() };
        Ok(Self {
            base: TextBase::new(handle, true, features)?,
        })
    }

    /// `callback` gets the whole text after each edit the user makes.
    pub fn with_callback<F>(callback: F) -> AureaResult<Self>
    where
        F: Fn(String) + 'static,
    {
        let editor = Self::new()?;
        editor
            .base
            .on_change(move |_, text| callback(text.to_owned()))?;
        Ok(editor)
    }

    /// Create a text editor with initial content.
    pub fn with_content(content: &str) -> AureaResult<Self> {
        let editor = Self::new()?;
        editor.set_text(content)?;
        Ok(editor)
    }
}
