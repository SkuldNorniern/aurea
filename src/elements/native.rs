//! Who owns a native element, and who frees it.

use crate::ffi::{
    ng_platform_creation_parent, ng_platform_destroy_element,
    ng_platform_element_released_to_parent, ng_platform_element_set_change_id,
    ng_platform_element_watch_destroy, ng_platform_free_text_content, ng_platform_get_text_content,
};
use crate::registry::custom::{
    next_custom_id, register_custom_callback, unregister_custom_callback,
};
use crate::{AureaError, AureaResult};
use std::cell::Cell;
use std::ffi::CStr;
use std::os::raw::{c_int, c_void};

pub(super) fn watch_destroy(
    handle: *mut c_void,
    callback: impl Fn() + 'static,
) -> AureaResult<CallbackWatch> {
    watch(handle, callback, ng_platform_element_watch_destroy)
}

fn watch(
    handle: *mut c_void,
    callback: impl Fn() + 'static,
    register: unsafe extern "C" fn(*mut c_void, u32) -> c_int,
) -> AureaResult<CallbackWatch> {
    let id = next_custom_id();
    if unsafe { register(handle, id) } != 0 {
        return Err(AureaError::ElementOperationFailed);
    }
    register_custom_callback(id, callback);
    Ok(CallbackWatch(id))
}

#[derive(Debug)]
pub(crate) struct CallbackWatch(u32);

impl Drop for CallbackWatch {
    fn drop(&mut self) {
        unregister_custom_callback(self.0);
    }
}

/// A native element handle, freed on drop unless a parent took it over.
#[derive(Debug)]
pub struct NativeElement {
    handle: *mut c_void,
    owned: Cell<bool>,
}

impl NativeElement {
    pub(crate) fn text(&self) -> AureaResult<String> {
        let ptr = unsafe { ng_platform_get_text_content(self.handle) };
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

    pub(crate) fn watch_change(
        &self,
        on_change: impl Fn() + 'static,
    ) -> AureaResult<CallbackWatch> {
        watch(self.handle, on_change, ng_platform_element_set_change_id)
    }

    /// What a custom native control is created under before a container adopts it: a hidden `HWND`
    /// on Windows, null elsewhere.
    pub fn creation_parent() -> *mut c_void {
        unsafe { ng_platform_creation_parent() }
    }

    /// Takes ownership of a handle the caller has just created.
    pub fn new(handle: *mut c_void) -> Self {
        Self {
            handle,
            owned: Cell::new(true),
        }
    }

    /// The raw handle, for FFI calls.
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }

    /// Whether this value still frees the element.
    pub fn owns(&self) -> bool {
        self.owned.get()
    }

    /// Hands the element to its new native parent, which will free it.
    pub fn released_to_parent(&self) {
        if !self.owned.replace(false) {
            return;
        }
        unsafe { ng_platform_element_released_to_parent(self.handle) };
    }

    /// Takes ownership back, for an element that has been detached from its parent and is once
    /// again on its own.
    pub fn reclaimed(&self) {
        self.owned.set(true);
    }

    /// Gives up the handle without freeing it, for a caller that has arranged its destruction some
    /// other way.
    pub fn leak(&self) -> *mut c_void {
        self.owned.set(false);
        self.handle
    }
}

impl Drop for NativeElement {
    fn drop(&mut self) {
        if !self.owned.get() {
            return;
        }
        unsafe { ng_platform_destroy_element(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;

    #[test]
    fn a_null_handle_is_safe_to_drop() {
        drop(NativeElement::new(null_mut()));
    }

    #[test]
    fn a_fresh_element_owns_its_handle() {
        let element = NativeElement::new(null_mut());
        assert!(element.owns());
    }

    #[test]
    fn releasing_to_a_parent_gives_up_ownership() {
        let element = NativeElement::new(null_mut());
        element.released_to_parent();

        assert!(!element.owns(), "the parent frees it now");
    }

    #[test]
    fn detaching_takes_ownership_back() {
        let element = NativeElement::new(null_mut());
        element.released_to_parent();
        element.reclaimed();

        assert!(element.owns());
    }

    #[test]
    fn leaking_hands_back_the_handle_and_drops_the_claim() {
        let element = NativeElement::new(null_mut());
        let handle = element.leak();

        assert_eq!(handle, null_mut());
        assert!(!element.owns());
    }
}
