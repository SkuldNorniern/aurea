//! Typed text without the `text-input` feature: none is delivered.

use std::os::raw::{c_char, c_void};

use crate::WindowEvent;

pub(crate) fn deliver(_: *mut c_void, _: *const c_char) {}

pub(crate) fn typed(_: String) -> Option<WindowEvent> {
    None
}
