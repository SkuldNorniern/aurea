pub(crate) mod callbacks;
mod exports;
mod logging;

pub use aurea_ffi::*;
pub use callbacks::*;
pub use exports::ng_process_frames;
pub use logging::*;

use crate::{AureaError, AureaResult};
use std::ffi::{CString, c_int};

/// A native call's status as a result: 0 and up succeed, `NG_ERROR_UNSUPPORTED` says this
/// platform has no `operation`, and anything else is a failed operation.
pub(crate) fn status(operation: &'static str, code: c_int) -> AureaResult<()> {
    match code {
        0.. => Ok(()),
        NG_ERROR_UNSUPPORTED => Err(AureaError::unsupported(operation)),
        _ => Err(AureaError::ElementOperationFailed),
    }
}

pub(crate) fn c_title(title: impl Into<Vec<u8>>) -> AureaResult<CString> {
    CString::new(title).map_err(|_| AureaError::InvalidTitle)
}

pub(crate) fn c_text(text: impl Into<Vec<u8>>) -> AureaResult<CString> {
    CString::new(text).map_err(|_| AureaError::InvalidText)
}
