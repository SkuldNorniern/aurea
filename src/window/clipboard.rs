//! Clipboard text, RGBA8 images and application byte formats, on the desktop and iOS.

use crate::Platform;
use crate::ffi::{RawClipboardEntry, c_text};
#[cfg(any(windows, target_os = "linux", target_os = "macos", target_os = "ios"))]
use crate::ffi::{
    ng_platform_clipboard_formats, ng_platform_clipboard_free, ng_platform_clipboard_read,
    ng_platform_clipboard_write,
};
use crate::ffi::{
    ng_platform_free_clipboard_text, ng_platform_get_clipboard_text, ng_platform_set_clipboard_text,
};
use crate::platform::ui_thread;
use crate::{AureaError, AureaResult};
use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::str;
#[cfg(any(windows, target_os = "linux", target_os = "macos", target_os = "ios"))]
use std::{ptr, slice};

mod image;
pub use image::ClipboardImage;

/// Borrowed custom-format data to publish alongside other representations.
#[derive(Clone, Copy, Debug)]
pub struct ClipboardData<'a> {
    /// Format name of the custom data.
    pub format: &'a str,
    /// The data bytes.
    pub bytes: &'a [u8],
}

/// Native error code when a read exceeds the caller's byte budget.
pub const READ_LIMIT_EXCEEDED: i32 = -6;
/// Windows reports this while another process has the clipboard open.
pub const CLIPBOARD_BUSY: i32 = -7;

#[cfg(any(windows, target_os = "linux", target_os = "macos", target_os = "ios"))]
struct NativeBytes(*mut u8);
#[cfg(any(windows, target_os = "linux", target_os = "macos", target_os = "ios"))]
impl Drop for NativeBytes {
    fn drop(&mut self) {
        #[cfg(any(windows, target_os = "linux", target_os = "macos", target_os = "ios"))]
        unsafe {
            ng_platform_clipboard_free(self.0);
        }
    }
}

fn native_bytes(format: Option<&CStr>, max_bytes: usize) -> AureaResult<Option<Vec<u8>>> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos", target_os = "ios"))]
    {
        let mut buffer = NativeBytes(ptr::null_mut());
        let mut len = 0;
        let result = unsafe {
            match format {
                Some(format) => {
                    ng_platform_clipboard_read(format.as_ptr(), max_bytes, &mut buffer.0, &mut len)
                }
                None => ng_platform_clipboard_formats(max_bytes, &mut buffer.0, &mut len),
            }
        };
        status(result)?;
        if buffer.0.is_null() {
            return Ok(None);
        }
        if len > max_bytes || len > isize::MAX as usize {
            return Err(limit_error());
        }
        let mut data = Vec::new();
        data.try_reserve_exact(len)
            .map_err(|_| AureaError::ElementOperationFailed)?;
        data.extend_from_slice(unsafe { slice::from_raw_parts(buffer.0, len) });
        Ok(Some(data))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos", target_os = "ios")))]
    {
        let _ = (format, max_bytes);
        status(-5)?;
        Ok(None)
    }
}

fn status(result: i32) -> AureaResult<()> {
    match result {
        0 => Ok(()),
        -5 => Err(AureaError::Unsupported {
            operation: "clipboard byte formats",
            platform: Platform::current(),
        }),
        code => Err(AureaError::PlatformError(code)),
    }
}
fn limit_error() -> AureaError {
    AureaError::PlatformError(READ_LIMIT_EXCEEDED)
}
fn format_name(format: &str) -> AureaResult<CString> {
    if format.is_empty() {
        return Err(AureaError::ElementOperationFailed);
    }
    CString::new(format).map_err(|_| AureaError::ElementOperationFailed)
}

/// Clears all clipboard representations on the UI thread.
pub fn clear_clipboard() -> AureaResult<()> {
    ui_thread::check("clear_clipboard");
    #[cfg(any(windows, target_os = "linux", target_os = "macos", target_os = "ios"))]
    {
        status(unsafe { ng_platform_clipboard_write(ptr::null(), 0) })
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos", target_os = "ios")))]
    {
        status(-5)
    }
}

/// Lists native clipboard formats, up to `max_bytes` of UTF-8 names.
pub fn clipboard_formats(max_bytes: usize) -> AureaResult<Vec<String>> {
    ui_thread::check("clipboard_formats");
    let bytes = native_bytes(None, max_bytes)?.unwrap_or_default();
    bytes
        .split(|&b| b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| {
            str::from_utf8(name)
                .map(str::to_owned)
                .map_err(|_| AureaError::ElementOperationFailed)
        })
        .collect()
}

/// Reads one representation.
pub fn clipboard_data(format: &str, max_bytes: usize) -> AureaResult<Option<Vec<u8>>> {
    ui_thread::check("clipboard_data");
    native_bytes(Some(&format_name(format)?), max_bytes)
}

/// Replaces the clipboard with all supplied representations in one operation.
pub fn set_clipboard_data(items: &[ClipboardData<'_>]) -> AureaResult<()> {
    ui_thread::check("set_clipboard_data");
    let names = validated_names(items)?;
    let entries: Vec<_> = items
        .iter()
        .zip(&names)
        .map(|(item, name)| RawClipboardEntry {
            format: name.as_ptr(),
            data: item.bytes.as_ptr(),
            len: item.bytes.len(),
        })
        .collect();
    #[cfg(any(windows, target_os = "linux", target_os = "macos", target_os = "ios"))]
    {
        status(unsafe { ng_platform_clipboard_write(entries.as_ptr(), entries.len()) })
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos", target_os = "ios")))]
    {
        let _ = entries;
        status(-5)
    }
}

fn validated_names(items: &[ClipboardData<'_>]) -> AureaResult<Vec<CString>> {
    if items.is_empty() {
        return Err(AureaError::ElementOperationFailed);
    }
    let mut seen = HashSet::new();
    seen.try_reserve(items.len())
        .map_err(|_| AureaError::ElementOperationFailed)?;
    items
        .iter()
        .map(|item| {
            if !seen.insert(item.format) {
                return Err(AureaError::ElementOperationFailed);
            }
            format_name(item.format)
        })
        .collect()
}

/// Reads PNG images, and on Windows common 24/32-bit DIB images.
pub fn clipboard_image(max_bytes: usize) -> AureaResult<Option<ClipboardImage>> {
    if let Some(bytes) = clipboard_data("image/png", max_bytes)? {
        return image::decode_png(&bytes, max_bytes).map(Some);
    }
    #[cfg(windows)]
    for format in ["CF_DIBV5", "CF_DIB"] {
        if let Some(bytes) = clipboard_data(format, max_bytes)? {
            return image::decode_dib(&bytes, max_bytes).map(Some);
        }
    }
    Ok(None)
}

/// Publishes PNG plus application payloads such as layers or an ICC profile.
pub fn set_clipboard_image(image: &ClipboardImage, extra: &[ClipboardData<'_>]) -> AureaResult<()> {
    ui_thread::check("set_clipboard_image");
    let png = image::encode_png(image)?;
    let mut items = Vec::with_capacity(extra.len().saturating_add(2));
    items.push(ClipboardData {
        format: "image/png",
        bytes: &png,
    });
    #[cfg(windows)]
    let dib = image::encode_dib(image)?;
    #[cfg(windows)]
    items.push(ClipboardData {
        format: "CF_DIBV5",
        bytes: &dib,
    });
    items.extend_from_slice(extra);
    set_clipboard_data(&items)
}

/// Read the OS clipboard as a UTF-8 string.
pub fn clipboard_text() -> Option<String> {
    ui_thread::check("clipboard_text");
    let ptr = unsafe { ng_platform_get_clipboard_text() };
    if ptr.is_null() {
        return None;
    }
    let text = unsafe {
        let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        ng_platform_free_clipboard_text(ptr);
        s
    };
    if text.is_empty() { None } else { Some(text) }
}

/// Write a UTF-8 string to the OS clipboard.
pub fn set_clipboard_text(text: &str) -> AureaResult<()> {
    ui_thread::check("set_clipboard_text");
    let cstr = c_text(text)?;
    match unsafe { ng_platform_set_clipboard_text(cstr.as_ptr()) } {
        0 => Ok(()),
        CLIPBOARD_BUSY => Err(AureaError::PlatformError(CLIPBOARD_BUSY)),
        _ => Err(AureaError::ElementOperationFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(windows, target_os = "linux"))]
    use std::thread::sleep;
    #[cfg(any(windows, target_os = "linux"))]
    use std::time::{Duration, Instant};

    #[cfg(any(windows, target_os = "linux"))]
    fn retry<T>(mut call: impl FnMut() -> AureaResult<T>) -> AureaResult<T> {
        let started = Instant::now();
        loop {
            match call() {
                Err(AureaError::PlatformError(CLIPBOARD_BUSY))
                    if started.elapsed() < Duration::from_secs(2) =>
                {
                    sleep(Duration::from_millis(1))
                }
                result => return result,
            }
        }
    }

    #[test]
    fn invalid_formats_and_duplicate_payloads_do_not_reach_the_native_clipboard() {
        for format in ["", "invalid\0name"] {
            assert!(format_name(format).is_err());
        }
        assert!(validated_names(&[]).is_err());
        let item = ClipboardData {
            format: "application/x-aurea-test",
            bytes: &[1, 2, 3],
        };
        assert!(validated_names(&[item, item]).is_err());
    }

    #[cfg(any(windows, target_os = "linux"))]
    struct Restore(Vec<(String, Vec<u8>)>);
    #[cfg(any(windows, target_os = "linux"))]
    impl Drop for Restore {
        fn drop(&mut self) {
            let items: Vec<_> = self
                .0
                .iter()
                .map(|(format, bytes)| ClipboardData { format, bytes })
                .collect();
            if items.is_empty() {
                let _ = retry(clear_clipboard);
            } else {
                let _ = retry(|| set_clipboard_data(&items));
            }
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn read_text() -> String {
        retry(|| clipboard_text().ok_or(AureaError::PlatformError(CLIPBOARD_BUSY)))
            .expect("read text after clipboard listeners finish")
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn exercise_text() {
        retry(|| set_clipboard_text("Aurea 한글 🙂")).expect("write text");
        assert_eq!(read_text(), "Aurea 한글 🙂");
        assert!(matches!(
            set_clipboard_text("a\0b"),
            Err(AureaError::InvalidText)
        ));
        assert_eq!(read_text(), "Aurea 한글 🙂");
        retry(|| set_clipboard_text("a\nb\nc")).expect("write lines");
        assert_eq!(read_text(), "a\nb\nc");
        #[cfg(windows)]
        {
            let raw = retry(|| clipboard_data("CF_UNICODETEXT", usize::MAX))
                .expect("read raw text")
                .expect("raw text");
            let wide: Vec<u16> = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| u16::from_le_bytes(*p))
                .collect();
            assert_eq!(
                String::from_utf16_lossy(&wide).trim_end_matches('\0'),
                "a\r\nb\r\nc"
            );
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    #[ignore = "uses a native desktop clipboard; run serially with --ignored"]
    fn native_clipboard_round_trips_images_formats_and_empty_payloads() {
        use crate::Window;
        let _window = Window::new("Clipboard test", 32, 32).expect("window");
        let mut saved = Vec::new();
        for format in retry(|| clipboard_formats(usize::MAX)).expect("formats") {
            if matches!(
                format.as_str(),
                "TARGETS" | "MULTIPLE" | "TIMESTAMP" | "SAVE_TARGETS" | "CF_BITMAP"
            ) {
                continue;
            }
            match retry(|| clipboard_data(&format, usize::MAX)) {
                Ok(Some(bytes)) => saved.push((format, bytes)),
                Ok(None) | Err(AureaError::Unsupported { .. }) => {}
                Err(error) => panic!("save clipboard {format}: {error:?}"),
            }
        }
        let _restore = Restore(saved);
        exercise_native_round_trip();
        exercise_text();
        #[cfg(windows)]
        {
            retry(|| {
                set_clipboard_data(&[ClipboardData {
                    format: "CF_UNICODETEXT",
                    bytes: &[b'x', 0],
                }])
            })
            .expect("unterminated fixture");
            assert_eq!(
                clipboard_text(),
                None,
                "bounded UTF-16 scan rejects missing terminators"
            );
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn exercise_native_round_trip() {
        let image = ClipboardImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 0, 0, 255, 0, 128],
        };
        let format = "application/x-aurea-native-test-v1";
        let empty = "application/x-aurea-empty-test-v1";
        let items = [
            ClipboardData {
                format,
                bytes: &[0, 1, 255, 0, 12],
            },
            ClipboardData {
                format: empty,
                bytes: &[],
            },
        ];
        #[cfg(windows)]
        {
            let dib = image::encode_dib(&image).expect("DIB fixture");
            retry(|| {
                set_clipboard_data(&[ClipboardData {
                    format: "CF_DIBV5",
                    bytes: &dib,
                }])
            })
            .expect("native bitmap without PNG");
            assert_eq!(
                retry(|| clipboard_image(4096)).expect("DIB fallback"),
                Some(image.clone())
            );
        }
        retry(|| set_clipboard_image(&image, &items)).expect("write image and custom formats");
        assert_eq!(
            retry(|| clipboard_image(4096)).expect("read image"),
            Some(image)
        );
        assert_eq!(
            retry(|| clipboard_data(format, 4096)).expect("read payload"),
            Some(vec![0, 1, 255, 0, 12])
        );
        assert_eq!(
            retry(|| clipboard_data(empty, 4096)).expect("read empty"),
            Some(vec![])
        );
        let formats = retry(|| clipboard_formats(4096)).expect("formats");
        assert!(formats.iter().any(|name| name == "image/png"));
        assert!(formats.iter().any(|name| name == format));
        exercise_native_limits(format);
        retry(|| set_clipboard_data(&[items[0]])).expect("replace provider");
        assert_eq!(
            retry(|| clipboard_data(empty, 4096)).expect("old format removed"),
            None
        );
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn exercise_native_limits(format: &str) {
        assert!(matches!(
            retry(|| clipboard_data(format, 4)),
            Err(AureaError::PlatformError(-6))
        ));
        assert!(matches!(
            retry(|| clipboard_formats(1)),
            Err(AureaError::PlatformError(-6))
        ));
    }
}
