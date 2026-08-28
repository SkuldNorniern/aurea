mod callback;
pub mod custom;
pub mod elements;
pub mod menu;
pub mod window;

use std::os::raw::c_void;

#[inline]
pub(crate) fn handle_key(handle: *mut c_void) -> usize {
    handle as usize
}

#[cfg(test)]
mod lifetime_tests;
