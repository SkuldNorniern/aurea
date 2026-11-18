//! Input method composition and the candidate window's place.

use std::os::raw::{c_char, c_int, c_void};

use crate::ffi::{ng_platform_window_set_ime_allowed, ng_platform_window_set_ime_cursor_area};
use crate::input::c_string;
use crate::window::native::plain_status;
use crate::window::push_window_event;
use crate::{AureaResult, Rect, WindowEvent};

/// Queues preedit `text` for `window`, with the cursor at byte `cursor` if that lands on a character.
pub(crate) fn deliver(window: *mut c_void, text: *const c_char, cursor: c_int) {
    let text = c_string(text).unwrap_or_default();
    let event = WindowEvent::ImePreedit {
        cursor: cursor_at(&text, cursor),
        text,
    };
    push_window_event(window, event);
}

/// The event for preedit `text` from a mobile keyboard, with the cursor at `cursor`.
pub(crate) fn composing(text: String, cursor: c_int) -> Option<WindowEvent> {
    Some(WindowEvent::ImePreedit {
        text,
        cursor: usize::try_from(cursor).ok(),
    })
}

/// Tells the input method of `handle` where the caret is, so its candidate window opens beside it.
pub(crate) fn set_cursor_area(handle: *mut c_void, area: Rect) -> AureaResult<()> {
    let result = unsafe {
        ng_platform_window_set_ime_cursor_area(handle, area.x, area.y, area.width, area.height)
    };
    plain_status(result)
}

/// Lets the input method of `handle` compose, or keeps it out so keys type as they are.
pub(crate) fn set_allowed(handle: *mut c_void, allowed: bool) -> AureaResult<()> {
    plain_status(unsafe { ng_platform_window_set_ime_allowed(handle, c_int::from(allowed)) })
}

fn cursor_at(text: &str, cursor: c_int) -> Option<usize> {
    let cursor = usize::try_from(cursor).ok()?;
    text.is_char_boundary(cursor).then_some(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preedit_cursor_must_land_on_a_character() {
        assert_eq!(cursor_at("한글", -1), None);
        assert_eq!(cursor_at("한글", 0), Some(0));
        assert_eq!(cursor_at("한글", 3), Some(3));
        assert_eq!(cursor_at("한글", 6), Some(6));
        assert_eq!(cursor_at("한글", 1), None);
        assert_eq!(cursor_at("한글", 7), None);
    }
}
