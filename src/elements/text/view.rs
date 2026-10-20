//! Multi-line text that wraps.

use super::base::TextBase;
use super::{TextControl, TextFeatures, text_widget};
use crate::AureaResult;
use crate::ffi::ng_platform_create_text_view;
use std::os::raw::c_int;

/// Several lines of text, wrapped at the edge: for prose, notes and logs.
pub struct TextView {
    base: TextBase,
}

text_widget!(TextView);

impl TextView {
    pub fn new(editable: bool) -> AureaResult<Self> {
        Self::with_features(TextFeatures::VIEW.with_read_only(!editable))
    }

    pub fn with_features(features: TextFeatures) -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_text_view(c_int::from(!features.read_only)) };
        Ok(Self {
            base: TextBase::new(handle, true, features)?,
        })
    }

    /// `callback` gets the whole text after each edit the user makes.
    pub fn with_callback<F>(editable: bool, callback: F) -> AureaResult<Self>
    where
        F: Fn(String) + 'static,
    {
        let view = Self::new(editable)?;
        view.base
            .on_change(move |_, text| callback(text.to_owned()))?;
        Ok(view)
    }

    /// Create a text view with initial content.
    pub fn with_content(editable: bool, content: &str) -> AureaResult<Self> {
        let view = Self::new(editable)?;
        view.set_text(content)?;
        Ok(view)
    }
}
