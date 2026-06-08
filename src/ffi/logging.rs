use std::ffi::CStr;
use std::os::raw::c_char;

#[inline]
fn with_log_message(msg: *const c_char, f: impl FnOnce(&str)) {
    if msg.is_null() {
        return;
    }

    let c_str = unsafe { CStr::from_ptr(msg) };
    if let Ok(s) = c_str.to_str() {
        f(s);
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_error(msg: *const c_char) {
    with_log_message(msg, |s| log::error!("{}", s));
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_warn(msg: *const c_char) {
    with_log_message(msg, |s| log::warn!("{}", s));
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_info(msg: *const c_char) {
    with_log_message(msg, |s| log::info!("{}", s));
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_debug(msg: *const c_char) {
    with_log_message(msg, |s| log::debug!("{}", s));
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_trace(msg: *const c_char) {
    with_log_message(msg, |s| log::trace!("{}", s));
}
