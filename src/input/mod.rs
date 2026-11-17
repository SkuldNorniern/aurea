//! Keyboard, pointer, text, pen, touch and file-drop input, and the shortcuts they drive.
//!
//! Each input feature in `Cargo.toml` builds one part. [`supported`] says whether a part is built
//! in on this platform.

#[cfg(any(feature = "text-input", feature = "ime", feature = "drop-files"))]
use std::os::raw::c_char;

#[cfg(feature = "drop-files")]
pub(crate) mod drop;
#[cfg(feature = "ime")]
pub(crate) mod ime;
pub(crate) mod keyboard;
pub mod keymap;
pub(crate) mod pen;
#[cfg(feature = "text-input")]
pub(crate) mod text;
#[cfg(all(feature = "touch", any(target_os = "ios", target_os = "android", test)))]
pub(crate) mod touch;

pub use keyboard::{keyboard_connected, keyboard_height, set_keyboard_follows_focus};
pub use pen::{PenMode, PenSource, pen_mode, pen_source, set_pen_mode};

/// An input feature, for [`supported`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// Typed text, `WindowEvent::TextInput`.
    TextInput,
    /// Input method composition, `WindowEvent::ImePreedit`, and the candidate window's place.
    Ime,
    /// `WindowEvent::RawMouseMotion` and cursor lock. Not on iOS or Android, which have no raw
    /// mouse deltas.
    RawMouse,
    /// Pen samples, batches, taps, pressure and tilt.
    Pen,
    /// Fingers: `WindowEvent::LongPress`, `FingerTap`, `PressCancelled`, and a finger as the mouse.
    Touch,
    /// `WindowEvent::Magnify`, `Rotate` and `Gesture`, from a trackpad or two fingers.
    Gestures,
    /// `WindowEvent::FilesHovered`, `FilesDropped` and `FilesHoverEnded`.
    DropFiles,
}

/// Whether `feature` is built in and works on this platform, so an app can ask at run time and
/// leave out what is not there.
///
/// A feature built in but missing on one machine, such as a pen with no tablet driver, still
/// says true: its events just do not come. Each feature is true wherever it is built, except
/// [`Feature::RawMouse`] on iOS and Android.
///
/// ```rust
/// use aurea::input::{Feature, supported};
///
/// if supported(Feature::Pen) {
///     // draw with the pen
/// }
/// ```
pub fn supported(feature: Feature) -> bool {
    let built = match feature {
        Feature::TextInput => cfg!(feature = "text-input"),
        Feature::Ime => cfg!(feature = "ime"),
        Feature::RawMouse => cfg!(feature = "raw-mouse"),
        Feature::Pen => cfg!(feature = "pen"),
        Feature::Touch => cfg!(feature = "touch"),
        Feature::Gestures => cfg!(feature = "gestures"),
        Feature::DropFiles => cfg!(feature = "drop-files"),
    };
    let platform = match feature {
        Feature::RawMouse => !cfg!(any(target_os = "ios", target_os = "android")),
        _ => true,
    };
    built && platform
}

/// The C string at `ptr` as text, or `None` for null or invalid UTF-8.
#[cfg(any(feature = "text-input", feature = "ime", feature = "drop-files"))]
pub(crate) fn c_string(ptr: *const c_char) -> Option<String> {
    use std::ffi::CStr;

    if ptr.is_null() {
        return None;
    }
    // SAFETY: the native layer passes a NUL-terminated string that lives for the call.
    let c_str = unsafe { CStr::from_ptr(ptr) };
    c_str.to_str().ok().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_follows_the_compiled_features() {
        assert_eq!(supported(Feature::TextInput), cfg!(feature = "text-input"));
        assert_eq!(supported(Feature::Ime), cfg!(feature = "ime"));
        assert_eq!(supported(Feature::Pen), cfg!(feature = "pen"));
        assert_eq!(supported(Feature::Touch), cfg!(feature = "touch"));
        assert_eq!(supported(Feature::Gestures), cfg!(feature = "gestures"));
        assert_eq!(supported(Feature::DropFiles), cfg!(feature = "drop-files"));
        assert_eq!(
            supported(Feature::RawMouse),
            cfg!(feature = "raw-mouse") && !cfg!(any(target_os = "ios", target_os = "android"))
        );
    }

    #[test]
    fn ime_needs_text_input() {
        assert!(!supported(Feature::Ime) || supported(Feature::TextInput));
    }
}
