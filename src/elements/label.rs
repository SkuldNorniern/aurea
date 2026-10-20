use super::native::NativeElement;
use super::traits::Element;
use crate::ffi::c_text;
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::os::raw::c_void;

/// How a label's text is set: weight and size from the platform's UI font.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextStyle {
    /// The platform's normal UI text.
    #[default]
    Body,
    /// Body size, semibold.
    Strong,
    /// Smaller than body.
    Small,
    /// Larger and semibold, for a page or section title.
    Heading,
}

/// A text label element.
pub struct Label {
    handle: NativeElement,
}

impl Label {
    /// Create a label with the given text.
    pub fn new(text: &str) -> AureaResult<Self> {
        let text = c_text(text)?;
        let handle = unsafe { ng_platform_create_label(text.as_ptr()) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self {
            handle: NativeElement::new(handle),
        })
    }

    /// A label holding `text` in `style`.
    pub fn with_style(text: &str, style: TextStyle) -> AureaResult<Self> {
        let mut label = Self::new(text)?;
        label.set_style(style)?;
        Ok(label)
    }

    /// Sets how the text is drawn; the layout makes room for it.
    pub fn set_style(&mut self, style: TextStyle) -> AureaResult<()> {
        let style = match style {
            TextStyle::Body => NG_TEXT_STYLE_BODY,
            TextStyle::Strong => NG_TEXT_STYLE_STRONG,
            TextStyle::Small => NG_TEXT_STYLE_SMALL,
            TextStyle::Heading => NG_TEXT_STYLE_HEADING,
        };
        let result = unsafe { ng_platform_label_set_style(self.handle.handle(), style) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Replaces the text; the layout makes room for it.
    pub fn set_text(&mut self, text: &str) -> AureaResult<()> {
        let text = c_text(text)?;
        let result = unsafe { ng_platform_label_set_text(self.handle.handle(), text.as_ptr()) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }
}

impl Element for Label {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {
        unsafe {
            ng_platform_label_invalidate(self.handle.handle());
        }
    }
}
