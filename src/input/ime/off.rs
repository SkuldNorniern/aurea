//! Input method composition without the `ime` feature: nothing is composed, and the candidate
//! window's place is not set.

use std::os::raw::{c_char, c_int, c_void};

use crate::{AureaResult, Rect, WindowEvent};

pub(crate) fn deliver(_: *mut c_void, _: *const c_char, _: c_int) {}

pub(crate) fn composing(_: String, _: c_int) -> Option<WindowEvent> {
    None
}

pub(crate) fn set_cursor_area(_: *mut c_void, _: Rect) -> AureaResult<()> {
    Ok(())
}

pub(crate) fn set_allowed(_: *mut c_void, _: bool) -> AureaResult<()> {
    Ok(())
}
