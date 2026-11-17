//! What one edit did, worked out from the text before and after it.

use std::ops::Range;

/// An edit by the user: `range` of the text before it, in UTF-8 bytes, became `inserted`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextChange {
    /// The replaced range, in UTF-8 bytes, of the text before the edit.
    pub range: Range<usize>,
    /// The text put in place of `range`.
    pub inserted: String,
    /// The control's revision after this edit; see
    /// [`TextControl::revision`](super::TextControl::revision).
    pub revision: u64,
}

pub(crate) fn diff(old: &str, new: &str) -> Option<(Range<usize>, String)> {
    if old == new {
        return None;
    }
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let room = (old.len() - start).min(new.len() - start);
    let mut tail = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - tail) || !new.is_char_boundary(new.len() - tail) {
        tail -= 1;
    }
    Some((
        start..old.len() - tail,
        new[start..new.len() - tail].to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(old: &str, (range, inserted): (Range<usize>, String)) -> String {
        let mut out = old.to_owned();
        out.replace_range(range, &inserted);
        out
    }

    #[test]
    fn typing_inserts_at_the_cursor() {
        assert_eq!(diff("helo", "hello"), Some((3..3, "l".into())));
    }

    #[test]
    fn deleting_removes_a_range() {
        assert_eq!(diff("hello world", "hello"), Some((5..11, String::new())));
    }

    #[test]
    fn a_repeated_letter_keeps_the_earliest_place() {
        assert_eq!(diff("aa", "aaa"), Some((2..2, "a".into())));
    }

    #[test]
    fn changes_stay_on_character_boundaries() {
        let change = diff("한", "핝");
        assert_eq!(change, Some((0..3, "핝".into())));
    }

    #[test]
    fn every_change_reapplies() {
        for (old, new) in [
            ("", "x"),
            ("x", ""),
            ("abc", "aXc"),
            ("🙂🙂", "🙂"),
            ("line\nnext", "line\n\nnext"),
        ] {
            let change = diff(old, new).unwrap_or_default();
            assert_eq!(apply(old, change), new, "{old:?} to {new:?}");
        }
        assert_eq!(diff("same", "same"), None);
    }
}
