pub(crate) mod callbacks;
mod exports;
mod logging;

pub use aurea_ffi::*;
pub use callbacks::*;
pub use exports::ng_process_frames;
pub use logging::*;

use crate::{AureaError, AureaResult};
use std::ffi::CString;

pub(crate) fn c_title(title: impl Into<Vec<u8>>) -> AureaResult<CString> {
    CString::new(title).map_err(|_| AureaError::InvalidTitle)
}

pub(crate) fn c_text(text: impl Into<Vec<u8>>) -> AureaResult<CString> {
    CString::new(text).map_err(|_| AureaError::InvalidText)
}
