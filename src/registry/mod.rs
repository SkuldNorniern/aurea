mod callback;
pub mod custom;
pub mod elements;
pub mod menu;
pub mod window;

use std::os::raw::c_void;

/// Map a raw platform handle to the key used by handle-keyed registries.
///
/// Correctness depends on the native layer never reusing a freed handle's
/// address before the corresponding registry entry is unregistered.
#[inline]
pub(crate) fn handle_key(handle: *mut c_void) -> usize {
    handle as usize
}
