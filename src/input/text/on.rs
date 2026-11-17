//! Typed text, without control characters.

use std::os::raw::{c_char, c_void};

use crate::WindowEvent;
use crate::input::c_string;
use crate::window::push_window_event;

/// Queues the text at `text` for `window`, without control characters.
pub(crate) fn deliver(window: *mut c_void, text: *const c_char) {
    if let Some(text) = c_string(text).and_then(printable) {
        push_window_event(window, WindowEvent::TextInput { text });
    }
}

/// The event for text the mobile keyboard typed; `None` when it is empty.
pub(crate) fn typed(text: String) -> Option<WindowEvent> {
    (!text.is_empty()).then_some(WindowEvent::TextInput { text })
}

fn printable(text: String) -> Option<String> {
    let text = if text.chars().any(char::is_control) {
        text.chars().filter(|c| !c.is_control()).collect()
    } else {
        text
    };
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_are_not_typed_text() {
        for key in ["\r", "\u{8}", "\t", "\u{1b}", "\u{1}", "\u{7f}"] {
            assert_eq!(printable(key.to_owned()), None, "{key:?}");
        }
        assert_eq!(printable("a\u{8}b".to_owned()).as_deref(), Some("ab"));
        assert_eq!(printable("한글 😀".to_owned()).as_deref(), Some("한글 😀"));
    }
}
