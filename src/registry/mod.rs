mod callback;
pub mod custom;
#[cfg(feature = "widgets")]
pub mod elements;
#[cfg(any(feature = "menu", feature = "tray"))]
pub mod menu;
pub mod window;

use std::os::raw::c_void;

#[inline]
pub(crate) fn handle_key(handle: *mut c_void) -> usize {
    handle as usize
}
