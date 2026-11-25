//! Native window calls shared by the window's methods and its lifecycle listener.

use super::close::CloseFlags;
use super::types::WindowId;
use crate::ffi::{
    ng_platform_get_scale_factor, ng_platform_window_get_position, ng_platform_window_get_size,
    ng_platform_window_request_close, ng_platform_window_set_close_guard,
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

/// Physical pixels a logical pixel takes now; 1 when the system gives nothing sensible.
pub(super) fn scale(handle: *mut c_void) -> f32 {
    let scale = unsafe { ng_platform_get_scale_factor(handle) };
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Physical pixels for `logical` at `scale`, rounded as the canvases size their buffers.
pub(super) fn canvas_pixels(logical: u32, scale: f32) -> u32 {
    let pixels = (f64::from(logical) * f64::from(scale)).round();
    // logical sizes are window sized; the product stays far inside u32
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = pixels.max(0.0) as u32;
    pixels
}

/// Native sizes are signed; anything past `i32::MAX` is clamped rather than wrapped negative.
pub(super) fn to_native_dim(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(windows)]
unsafe extern "C" {
    fn ng_windows_window_set_owner(hwnd: *mut c_void, owner: u64);
    fn ng_windows_window_owned_by(hwnd: *mut c_void, owner: u64) -> c_int;
}

/// Ties the native window to its id, so a later drop can tell it is still that window.
pub(super) fn set_owner(handle: *mut c_void, id: WindowId) {
    #[cfg(windows)]
    unsafe {
        ng_windows_window_set_owner(handle, id.get());
    }
    #[cfg(not(windows))]
    let _ = (handle, id);
}

/// False once the platform has closed the window and Windows gave its handle value to another
/// window: destroying the handle would then destroy that one.
pub(super) fn still_ours(handle: *mut c_void, id: WindowId) -> bool {
    #[cfg(windows)]
    return unsafe { ng_windows_window_owned_by(handle, id.get()) } != 0;
    #[cfg(not(windows))]
    {
        let _ = (handle, id);
        true
    }
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
pub(crate) fn plain_status(code: c_int) -> AureaResult<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(AureaError::ElementOperationFailed)
    }
}
