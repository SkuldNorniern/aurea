//! Codes shared with the native side: `common/errors.h` and the text constants in
//! `common/platform_api.h`.

use std::os::raw::c_int;

/// Operation succeeded.
pub const NG_SUCCESS: c_int = 0;
/// Handle is null or stale.
pub const NG_ERROR_INVALID_HANDLE: c_int = -1;
/// Native object could not be created.
pub const NG_ERROR_CREATION_FAILED: c_int = -2;
/// Argument is invalid or out of range.
pub const NG_ERROR_INVALID_PARAMETER: c_int = -3;
/// Platform call failed with its own error.
pub const NG_ERROR_PLATFORM_SPECIFIC: c_int = -4;
/// The backend has no implementation for this: a statement about the platform, not a failure.
pub const NG_ERROR_UNSUPPORTED: c_int = -5;
/// A platform or size limit was reached.
pub const NG_ERROR_LIMIT_EXCEEDED: c_int = -6;
/// Resource is busy; retry later.
pub const NG_ERROR_BUSY: c_int = -7;

/// Body text style.
pub const NG_TEXT_STYLE_BODY: c_int = 0;
/// Strong text style.
pub const NG_TEXT_STYLE_STRONG: c_int = 1;
/// Small text style.
pub const NG_TEXT_STYLE_SMALL: c_int = 2;
/// Heading text style.
pub const NG_TEXT_STYLE_HEADING: c_int = 3;
/// Text offsets count UTF-16 code units.
pub const NG_TEXT_UNIT_UTF16: c_int = 0;
/// Text offsets count characters.
pub const NG_TEXT_UNIT_CHAR: c_int = 1;
/// Read-only text feature.
pub const NG_TEXT_FEATURE_READ_ONLY: c_int = 0;
/// Word wrap text feature.
pub const NG_TEXT_FEATURE_WRAP: c_int = 1;
/// Monospace text feature.
pub const NG_TEXT_FEATURE_MONOSPACE: c_int = 2;
/// Undo text feature.
pub const NG_TEXT_FEATURE_UNDO: c_int = 3;
