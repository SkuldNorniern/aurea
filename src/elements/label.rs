use super::native::NativeElement;
use super::traits::Element;
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

pub struct Label {
    handle: NativeElement,
}

impl Label {
    pub fn new(text: &str) -> AureaResult<Self> {
        let text = CString::new(text).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_label(text.as_ptr()) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self {
            handle: NativeElement::new(handle),
        })
    }

    /// Replaces the text; the layout makes room for it.
    pub fn set_text(&mut self, text: &str) -> AureaResult<()> {
        let text = CString::new(text).map_err(|_| AureaError::InvalidTitle)?;
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
