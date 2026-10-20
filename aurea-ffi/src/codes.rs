//! Codes shared with the native side: `common/errors.h` and the text constants in
//! `common/platform_api.h`.

use std::os::raw::c_int;

pub const NG_SUCCESS: c_int = 0;
pub const NG_ERROR_INVALID_HANDLE: c_int = -1;
pub const NG_ERROR_CREATION_FAILED: c_int = -2;
pub const NG_ERROR_INVALID_PARAMETER: c_int = -3;
pub const NG_ERROR_PLATFORM_SPECIFIC: c_int = -4;
/// The backend has no implementation for this: a statement about the platform, not a failure.
pub const NG_ERROR_UNSUPPORTED: c_int = -5;
pub const NG_ERROR_LIMIT_EXCEEDED: c_int = -6;
pub const NG_ERROR_BUSY: c_int = -7;

pub const NG_TEXT_STYLE_BODY: c_int = 0;
pub const NG_TEXT_STYLE_STRONG: c_int = 1;
pub const NG_TEXT_STYLE_SMALL: c_int = 2;
pub const NG_TEXT_STYLE_HEADING: c_int = 3;
pub const NG_TEXT_UNIT_UTF16: c_int = 0;
pub const NG_TEXT_UNIT_CHAR: c_int = 1;
pub const NG_TEXT_FEATURE_READ_ONLY: c_int = 0;
pub const NG_TEXT_FEATURE_WRAP: c_int = 1;
pub const NG_TEXT_FEATURE_MONOSPACE: c_int = 2;
pub const NG_TEXT_FEATURE_UNDO: c_int = 3;
