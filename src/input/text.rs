//! Typed text, as `WindowEvent::TextInput`.

use std::os::raw::c_char;

use super::c_string;
use crate::WindowEvent;

/// The event for the text at `text`, without control characters; `None` when nothing is left.
pub(crate) fn typed(text: *const c_char) -> Option<WindowEvent> {
    let text = printable(c_string(text)?)?;
    Some(WindowEvent::TextInput { text })
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
