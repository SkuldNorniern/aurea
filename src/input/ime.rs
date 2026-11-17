//! Input method composition, as `WindowEvent::ImePreedit`.

use std::os::raw::{c_char, c_int};

use super::c_string;
use crate::WindowEvent;

/// The preedit event for `text`, with the cursor at byte `cursor` if that lands on a character.
pub(crate) fn preedit(text: *const c_char, cursor: c_int) -> WindowEvent {
    let text = c_string(text).unwrap_or_default();
    WindowEvent::ImePreedit {
        cursor: cursor_at(&text, cursor),
        text,
    }
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
