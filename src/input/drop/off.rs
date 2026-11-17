//! File drops without the `drop-files` feature: none are delivered.

use std::os::raw::{c_char, c_void};

pub(crate) fn deliver(_: *mut c_void, _: u32, _: f64, _: f64, _: *const c_char) {}

#[cfg(target_os = "ios")]
pub(crate) unsafe fn dropped(_: *const c_char, _: f64, _: f64) -> Option<crate::WindowEvent> {
    None
}
