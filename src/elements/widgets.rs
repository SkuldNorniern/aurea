//! What the native controls share: reading their text, watching their changes, and the spacer
//! a stack adds. Only with `widgets`.

use std::ffi::CStr;
use std::os::raw::c_void;

use super::native::CallbackWatch;
use super::{Container, NativeElement, Spacer, Stack};
use crate::ffi::{
    ng_platform_element_set_change_id, ng_platform_free_text_content, ng_platform_get_text_content,
};
use crate::{AureaError, AureaResult};

pub(crate) fn read_text(handle: *mut c_void) -> AureaResult<String> {
    let ptr = unsafe { ng_platform_get_text_content(handle) };
    if ptr.is_null() {
        return Err(AureaError::ElementOperationFailed);
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(str::to_owned)
        .map_err(|_| AureaError::ElementOperationFailed);
    unsafe { ng_platform_free_text_content(ptr) };
    text
}

impl NativeElement {
    pub(crate) fn watch_change(
        &self,
        on_change: impl Fn() + 'static,
    ) -> AureaResult<CallbackWatch> {
        Self::watch_change_of(self.handle(), on_change)
    }

    /// [`Self::watch_change`] for a handle a container took over.
    pub(crate) fn watch_change_of(
        handle: *mut c_void,
        on_change: impl Fn() + 'static,
    ) -> AureaResult<CallbackWatch> {
        super::native::watch(handle, on_change, ng_platform_element_set_change_id)
    }
}

impl Stack {
    /// Add a spacer that expands with the given weight.
    pub fn add_spacer(&mut self, weight: f32) -> AureaResult<()> {
        self.add_weighted(Spacer::new()?, weight)
    }
}
