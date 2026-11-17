//! Clipboard text, RGBA8 images and application byte formats. With `clipboard` they reach the
//! native clipboard; without it, reads come back empty and writes and byte formats are
//! `Unsupported`.

use crate::image;
pub use crate::image::ClipboardImage;

#[cfg(not(feature = "clipboard"))]
mod off;
#[cfg(feature = "clipboard")]
mod on;

#[cfg(not(feature = "clipboard"))]
pub use off::{
    clear_clipboard, clipboard_data, clipboard_formats, clipboard_image, clipboard_text,
    set_clipboard_data, set_clipboard_image, set_clipboard_text,
};
#[cfg(feature = "clipboard")]
pub use on::{
    clear_clipboard, clipboard_data, clipboard_formats, clipboard_image, clipboard_text,
    set_clipboard_data, set_clipboard_image, set_clipboard_text,
};

/// Borrowed custom-format data to publish alongside other representations.
#[derive(Clone, Copy, Debug)]
pub struct ClipboardData<'a> {
    /// Format name of the custom data.
    pub format: &'a str,
    /// The data bytes.
    pub bytes: &'a [u8],
}

/// Native error code when a read exceeds the caller's byte budget.
pub const READ_LIMIT_EXCEEDED: i32 = image::READ_LIMIT_EXCEEDED;
/// Windows reports this while another process has the clipboard open.
pub const CLIPBOARD_BUSY: i32 = -7;

/// Whether the clipboard is built in. Text fields ask it, and they are absent without `ui-core`.
#[allow(dead_code)]
pub(crate) const fn built() -> bool {
    cfg!(feature = "clipboard")
}
