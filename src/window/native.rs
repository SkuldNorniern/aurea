//! Native window calls shared by the window's methods and its lifecycle listener.

use super::close::CloseFlags;
use crate::ffi::{
    ng_platform_window_get_position, ng_platform_window_get_size, ng_platform_window_request_close,
    ng_platform_window_set_close_guard,
};
use crate::{AureaError, AureaResult};
use std::os::raw::{c_int, c_void};

pub(super) fn position(handle: *mut c_void) -> (i32, i32) {
    let mut x = 0;
    let mut y = 0;
    unsafe {
        ng_platform_window_get_position(handle, &mut x, &mut y);
    }
    (x, y)
}

pub(super) fn size(handle: *mut c_void) -> (i32, i32) {
    let mut width = 0;
    let mut height = 0;
    unsafe {
        ng_platform_window_get_size(handle, &mut width, &mut height);
    }
    (width, height)
}

/// Native sizes are signed; anything past `i32::MAX` is clamped rather than wrapped negative.
pub(super) fn to_native_dim(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub(super) fn set_close_guard(handle: *mut c_void, guarded: bool) {
    unsafe {
        ng_platform_window_set_close_guard(handle, i32::from(guarded));
    }
}

/// Closes past the guard. Marks the close as the window's own, so the platform's will-close
/// is not reported as a request again.
pub(super) fn force_close(handle: *mut c_void, flags: &CloseFlags) {
    set_close_guard(handle, false);
    flags.mark_forced();
    unsafe {
        ng_platform_window_request_close(handle);
    }
}

/// The `0 = ok` status most window setters return.
pub(super) fn plain_status(code: c_int) -> AureaResult<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(AureaError::ElementOperationFailed)
    }
}
