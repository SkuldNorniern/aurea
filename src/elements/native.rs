//! Who owns a native element, and who frees it.
//!
//! The rule is that a Rust value owns the native element it created, and frees
//! it when dropped. Adding a child to a container hands that ownership over,
//! because the platform frees a container's children along with it — on
//! Windows `DestroyWindow` takes the child HWNDs with it, and the other
//! backends behave the same way. Without the handover the child would be freed
//! twice: once by its parent and once by its own `Drop`.
//!
//! Elements embed a [`NativeElement`] rather than a bare pointer, so the rule
//! is written down once instead of in every widget's `Drop`.

use crate::ffi::ng_platform_destroy_element;
use std::cell::Cell;
use std::os::raw::c_void;

/// A native element handle, freed on drop unless a parent took it over.
#[derive(Debug)]
pub struct NativeElement {
    handle: *mut c_void,
    /// False once a container has taken responsibility for freeing it.
    owned: Cell<bool>,
}

impl NativeElement {
    /// Takes ownership of a handle the caller has just created.
    pub fn new(handle: *mut c_void) -> Self {
        Self {
            handle,
            owned: Cell::new(true),
        }
    }

    /// The raw handle, for FFI calls. Borrowing it does not transfer anything.
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }

    /// Whether this value still frees the element.
    pub fn owns(&self) -> bool {
        self.owned.get()
    }

    /// Hands the element to its new native parent, which will free it.
    ///
    /// Called when a container adopts a child. The Rust value stays alive and
    /// usable — it just no longer frees anything.
    pub fn released_to_parent(&self) {
        self.owned.set(false);
    }

    /// Takes ownership back, for an element that has been detached from its
    /// parent and is once again on its own.
    pub fn reclaimed(&self) {
        self.owned.set(true);
    }

    /// Gives up the handle without freeing it, for a caller that has arranged
    /// its destruction some other way.
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

    /// A null handle is what a failed creation leaves behind, and destroying it
    /// must be harmless.
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
