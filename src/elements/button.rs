use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

pub struct Button {
    handle: *mut c_void,
    _title: CString,
    _id: u32,
}

impl Button {
    pub fn new(title: &str) -> AureaResult<Self> {
        Self::with_callback(title, || {})
    }

    pub fn with_callback<F>(title: &str, callback: F) -> AureaResult<Self>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let id = crate::registry::elements::next_button_id();

        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_button(title.as_ptr(), id) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        crate::registry::elements::register_button_callback(id, callback);

        Ok(Self {
            handle,
            _title: title,
            _id: id,
        })
    }
}

pub(crate) fn invoke_button_callback(id: u32) {
    crate::registry::elements::invoke_button_callback(id);
}

impl Element for Button {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_button_invalidate(self.handle);
        }
    }
}
