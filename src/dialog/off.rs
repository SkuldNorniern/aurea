//! The native dialog calls without `dialog`. They have the same names and signatures as the `ffi`
//! calls. `run` refuses before any is made, so these only report that the platform has no dialog.

use super::NG_ERROR_UNSUPPORTED;
use std::ffi::{c_char, c_int, c_uint};
use std::os::raw::c_void;
use std::ptr::null_mut;

pub(super) unsafe fn ng_platform_file_dialog(
    _: *mut c_void,
    _: c_int,
    _: *const c_char,
    _: *const c_char,
    _: *const c_char,
    _: *const c_char,
    _: c_int,
) -> *mut c_char {
    null_mut()
}

pub(super) unsafe fn ng_platform_free_dialog_paths(_: *mut c_char) {}

pub(super) unsafe fn ng_platform_message_box(
    _: *mut c_void,
    _: c_int,
    _: *const c_char,
    _: *const c_char,
    _: c_int,
) -> c_int {
    NG_ERROR_UNSUPPORTED
}

pub(super) unsafe fn ng_platform_color_dialog(
    _: *mut c_void,
    _: *const c_char,
    _: c_uint,
    _: c_int,
    _: *mut c_uint,
) -> c_int {
    NG_ERROR_UNSUPPORTED
}
