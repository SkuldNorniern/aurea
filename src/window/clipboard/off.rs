//! The clipboard without `clipboard`: nothing is read or written, and byte formats are unsupported.

use super::ClipboardData;
use crate::{AureaError, AureaResult, image::ClipboardImage};

fn unsupported() -> AureaError {
    AureaError::unsupported("clipboard")
}

/// Clears all clipboard representations.
pub fn clear_clipboard() -> AureaResult<()> {
    Err(unsupported())
}

/// Lists native clipboard formats.
pub fn clipboard_formats(_max_bytes: usize) -> AureaResult<Vec<String>> {
    Err(unsupported())
}

/// Reads one representation.
pub fn clipboard_data(_format: &str, _max_bytes: usize) -> AureaResult<Option<Vec<u8>>> {
    Err(unsupported())
}

/// Replaces the clipboard with all supplied representations.
pub fn set_clipboard_data(_items: &[ClipboardData<'_>]) -> AureaResult<()> {
    Err(unsupported())
}

/// Reads PNG images, and on Windows common 24/32-bit DIB images.
pub fn clipboard_image(_max_bytes: usize) -> AureaResult<Option<ClipboardImage>> {
    Err(unsupported())
}

/// Publishes a PNG plus application payloads.
pub fn set_clipboard_image(
    _image: &ClipboardImage,
    _extra: &[ClipboardData<'_>],
) -> AureaResult<()> {
    Err(unsupported())
}

/// Read the OS clipboard as a UTF-8 string; none without `clipboard`.
pub fn clipboard_text() -> Option<String> {
    None
}

/// Write a UTF-8 string to the OS clipboard.
pub fn set_clipboard_text(_text: &str) -> AureaResult<()> {
    Err(unsupported())
}
