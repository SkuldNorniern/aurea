//! The switches every text control shares.

use crate::ffi::{
    NG_TEXT_FEATURE_MONOSPACE, NG_TEXT_FEATURE_READ_ONLY, NG_TEXT_FEATURE_UNDO,
    NG_TEXT_FEATURE_WRAP,
};
use std::os::raw::c_int;

/// What a text control does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextFeatures {
    /// The text cannot be changed by the user.
    pub read_only: bool,
    /// Long lines wrap at the edge instead of scrolling sideways.
    pub wrap: bool,
    /// A fixed-width face, for source and logs.
    pub monospace: bool,
    /// Edits can be undone and redone, by keyboard and by
    /// [`TextControl::undo`](super::TextControl::undo).
    pub undo: bool,
}

impl TextFeatures {
    /// [`TextField`](super::TextField): one line of input.
    pub const FIELD: Self = Self {
        read_only: false,
        wrap: false,
        monospace: false,
        undo: true,
    };
    /// [`TextView`](super::TextView): prose that wraps.
    pub const VIEW: Self = Self {
        read_only: false,
        wrap: true,
        monospace: false,
        undo: true,
    };
    /// [`TextEditor`](super::TextEditor): source, lines running on.
    pub const EDITOR: Self = Self {
        read_only: false,
        wrap: false,
        monospace: true,
        undo: true,
    };

    /// Returns these features with `read_only` set to `on`.
    #[must_use]
    pub const fn with_read_only(mut self, on: bool) -> Self {
        self.read_only = on;
        self
    }

    /// Returns these features with `wrap` set to `on`.
    #[must_use]
    pub const fn with_wrap(mut self, on: bool) -> Self {
        self.wrap = on;
        self
    }

    /// Returns these features with `monospace` set to `on`.
    #[must_use]
    pub const fn with_monospace(mut self, on: bool) -> Self {
        self.monospace = on;
        self
    }

    /// Returns these features with `undo` set to `on`.
    #[must_use]
    pub const fn with_undo(mut self, on: bool) -> Self {
        self.undo = on;
        self
    }

    pub(crate) fn switches(self, multiline: bool) -> Vec<(c_int, bool)> {
        let mut out = vec![
            (NG_TEXT_FEATURE_READ_ONLY, self.read_only),
            (NG_TEXT_FEATURE_MONOSPACE, self.monospace),
        ];
        if multiline {
            out.push((NG_TEXT_FEATURE_WRAP, self.wrap));
            out.push((NG_TEXT_FEATURE_UNDO, self.undo));
        }
        out
    }

    pub(crate) fn changed_from(self, old: Self, multiline: bool) -> Vec<(c_int, bool)> {
        let before = old.switches(multiline);
        self.switches(multiline)
            .into_iter()
            .zip(before)
            .filter(|(now, was)| now != was)
            .map(|(now, _)| now)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_has_no_wrap_or_undo_switch() {
        let ids: Vec<_> = TextFeatures::FIELD
            .switches(false)
            .into_iter()
            .map(|s| s.0)
            .collect();
        assert_eq!(ids, [NG_TEXT_FEATURE_READ_ONLY, NG_TEXT_FEATURE_MONOSPACE]);
    }

    #[test]
    fn only_changed_switches_are_sent() {
        let now = TextFeatures::EDITOR.with_wrap(true);
        assert_eq!(
            now.changed_from(TextFeatures::EDITOR, true),
            [(NG_TEXT_FEATURE_WRAP, true)]
        );
    }
}
