//! The native menu calls without `menu`. They have the same names and signatures as the `ffi`
//! calls, and report failure: no menu bar is ever created, so nothing is drawn.

use std::ffi::{c_char, c_int};
use std::os::raw::c_void;
use std::ptr::null_mut;

pub(super) unsafe fn ng_platform_create_menu() -> *mut c_void {
    null_mut()
}

pub(super) unsafe fn ng_platform_destroy_menu(_: *mut c_void) {}

pub(super) unsafe fn ng_platform_attach_menu(_: *mut c_void, _: *mut c_void) -> c_int {
    -1
}

pub(super) unsafe fn ng_platform_add_menu_item(_: *mut c_void, _: *const c_char, _: u32) -> c_int {
    -1
}

pub(super) unsafe fn ng_platform_add_menu_separator(_: *mut c_void) -> c_int {
    -1
}

pub(super) unsafe fn ng_platform_create_submenu(_: *mut c_void, _: *const c_char) -> *mut c_void {
    null_mut()
}

pub(super) unsafe fn ng_platform_add_menu_check_item(
    _: *mut c_void,
    _: *const c_char,
    _: u32,
    _: c_int,
    _: c_int,
) -> c_int {
    -1
}

pub(super) unsafe fn ng_platform_menu_item_set_enabled(_: *mut c_void, _: u32, _: c_int) -> c_int {
    -1
}

pub(super) unsafe fn ng_platform_menu_item_set_checked(_: *mut c_void, _: u32, _: c_int) -> c_int {
    -1
}

pub(super) unsafe fn ng_platform_menu_item_set_title(
    _: *mut c_void,
    _: u32,
    _: *const c_char,
) -> c_int {
    -1
}

pub(super) unsafe fn ng_platform_menu_item_remove(_: *mut c_void, _: u32) -> c_int {
    -1
}

pub(super) unsafe fn ng_platform_menu_clear(_: *mut c_void) -> c_int {
    -1
}

pub(super) unsafe fn ng_platform_menu_move_last(_: *mut c_void, _: u32) -> c_int {
    -1
}
