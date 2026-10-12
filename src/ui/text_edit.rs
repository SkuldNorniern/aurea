//! Editing one line of text: a caret, a selection, what an input method is
//! still composing, and undo.
//!
//! Pure logic over a `String` the caller owns, so it is tested without a
//! window. Positions are byte offsets that always sit on a character
//! boundary; [`TextEdit::clamp`] puts them back on one if the text changed
//! underneath.

use std::mem;
use std::ops::Range;

/// How far a caret motion or a deletion goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// One character.
    Char,
    /// To the next word boundary.
    Word,
    /// To the start or end of the line.
    Line,
}

/// Which way a caret motion or a deletion goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Back,
    Forward,
}

/// What a field takes. Text that breaks a rule is cut down to what fits,
/// not refused whole, so a long paste fills the field.
#[derive(Clone, Copy, Default)]
pub struct Rules<'a> {
    /// Most characters the text may hold.
    pub max_chars: Option<usize>,
    /// Which characters may be typed or pasted; others are dropped.
    pub accept: Option<&'a dyn Fn(char) -> bool>,
}

/// Undo keeps this many steps; older ones are dropped.
const HISTORY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    None,
    Typing,
    Deleting,
}

#[derive(Clone, Debug)]
struct Snapshot {
    text: String,
    caret: usize,
    anchor: usize,
}

/// The editing state of one text field.
#[derive(Clone, Debug)]
pub struct TextEdit {
    caret: usize,
    anchor: usize,
    preedit: String,
    preedit_caret: Option<usize>,
    /// How far the text is scrolled left, in pixels, to keep the caret in
    /// sight. Kept by the field that draws it.
    pub scroll: f32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Kind,
}

impl Default for TextEdit {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEdit {
    pub const fn new() -> Self {
        Self {
            caret: 0,
            anchor: 0,
            preedit: String::new(),
            preedit_caret: None,
            scroll: 0.0,
            undo: Vec::new(),
            redo: Vec::new(),
            last: Kind::None,
        }
    }

    /// Where the caret is.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selected range, start first; empty when nothing is selected.
    pub fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    pub fn has_selection(&self) -> bool {
        self.caret != self.anchor
    }

    /// What is selected in `text`.
    pub fn selected<'t>(&self, text: &'t str) -> &'t str {
        text.get(self.selection()).unwrap_or("")
    }

    /// What the input method is composing, shown at the caret, not yet in
    /// the text.
    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    /// The input method's caret inside [`Self::preedit`], if it shows one.
    pub fn preedit_caret(&self) -> Option<usize> {
        self.preedit_caret
    }

    /// Puts both ends back inside `text` and on character boundaries, for
    /// text the app changed itself.
    pub fn clamp(&mut self, text: &str) {
        self.caret = floor_boundary(text, self.caret);
        self.anchor = floor_boundary(text, self.anchor);
    }

    /// Moves the caret to `at`, keeping the selection's other end when
    /// `extend`.
    pub fn set_caret(&mut self, text: &str, at: usize, extend: bool) {
        self.caret = floor_boundary(text, at);
        if !extend {
            self.anchor = self.caret;
        }
        self.last = Kind::None;
    }

    pub fn select_all(&mut self, text: &str) {
        self.anchor = 0;
        self.caret = text.len();
        self.last = Kind::None;
    }

    /// Selects the word, or the run of spaces or punctuation, around `at`.
    pub fn select_word_at(&mut self, text: &str, at: usize) {
        let at = floor_boundary(text, at);
        let class = text[at..]
            .chars()
            .next()
            .or_else(|| text[..at].chars().next_back())
            .map_or(Class::Space, class_of);
        let start = text[..at]
            .char_indices()
            .rev()
            .take_while(|&(_, c)| class_of(c) == class)
            .last()
            .map_or(at, |(i, _)| i);
        let end = text[at..]
            .char_indices()
            .find(|&(_, c)| class_of(c) != class)
            .map_or(text.len(), |(i, _)| at + i);
        self.anchor = start;
        self.caret = end;
        self.last = Kind::None;
    }

    /// Moves the caret by `step` in `direction`. Without `extend`, a
    /// selection collapses to its near end instead of moving a character.
    pub fn move_caret(&mut self, text: &str, step: Step, direction: Direction, extend: bool) {
        self.last = Kind::None;
        if !extend && self.has_selection() && step == Step::Char {
            let Range { start, end } = self.selection();
            self.caret = if direction == Direction::Back {
                start
            } else {
                end
            };
            self.anchor = self.caret;
            return;
        }
        self.caret = target(text, self.caret, step, direction);
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Types `typed` over the selection, kept to `rules`. Returns whether
    /// the text changed.
    pub fn insert(&mut self, text: &mut String, typed: &str, rules: Rules<'_>) -> bool {
        let kept = allowed(text, self.selection(), typed, rules);
        if kept.is_empty() && !self.has_selection() {
            return false;
        }
        let word_break = kept.chars().any(char::is_whitespace);
        self.remember(text, if word_break { Kind::None } else { Kind::Typing });
        let range = self.selection();
        text.replace_range(range.clone(), &kept);
        self.caret = range.start + kept.len();
        self.anchor = self.caret;
        true
    }

    /// Deletes the selection, or else `step` in `direction` from the
    /// caret. Returns whether the text changed.
    pub fn delete(&mut self, text: &mut String, step: Step, direction: Direction) -> bool {
        let range = if self.has_selection() {
            self.selection()
        } else {
            let to = target(text, self.caret, step, direction);
            self.caret.min(to)..self.caret.max(to)
        };
        if range.is_empty() {
            return false;
        }
        self.remember(text, Kind::Deleting);
        text.replace_range(range.clone(), "");
        self.caret = range.start;
        self.anchor = range.start;
        true
    }

    /// Takes the selection out of `text` and returns it, for cut.
    pub fn cut(&mut self, text: &mut String) -> Option<String> {
        if !self.has_selection() {
            return None;
        }
        let taken = self.selected(text).to_string();
        self.delete(text, Step::Char, Direction::Back);
        Some(taken)
    }

    /// Sets what the input method is composing; empty ends it. A
    /// composition replaces the selection, as typing would.
    pub fn set_preedit(&mut self, text: &mut String, preedit: &str, caret: Option<usize>) {
        if !preedit.is_empty() && self.has_selection() {
            self.delete(text, Step::Char, Direction::Back);
        }
        self.preedit.clear();
        self.preedit.push_str(preedit);
        self.preedit_caret = caret.filter(|&c| preedit.is_char_boundary(c));
    }

    /// Goes back one step. Returns whether there was one.
    pub fn undo(&mut self, text: &mut String) -> bool {
        self.step_history(text, true)
    }

    /// Goes forward again after [`Self::undo`].
    pub fn redo(&mut self, text: &mut String) -> bool {
        self.step_history(text, false)
    }

    fn step_history(&mut self, text: &mut String, back: bool) -> bool {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(snapshot) = from.pop() else {
            return false;
        };
        to.push(Snapshot {
            text: mem::replace(text, snapshot.text),
            caret: self.caret,
            anchor: self.anchor,
        });
        self.caret = floor_boundary(text, snapshot.caret);
        self.anchor = floor_boundary(text, snapshot.anchor);
        self.last = Kind::None;
        true
    }

    /// Saves the text before an edit, unless it continues the last one:
    /// typing a word, or deleting, undoes as one step.
    fn remember(&mut self, text: &str, kind: Kind) {
        let continues = kind != Kind::None && kind == self.last && !self.has_selection();
        self.last = kind;
        if continues {
            return;
        }
        if self.undo.len() == HISTORY {
            self.undo.remove(0);
        }
        self.undo.push(Snapshot {
            text: text.to_string(),
            caret: self.caret,
            anchor: self.anchor,
        });
        self.redo.clear();
    }
}

/// `typed` cut to what `rules` let into `text` once `replacing` is gone.
fn allowed(text: &str, replacing: Range<usize>, typed: &str, rules: Rules<'_>) -> String {
    let left = rules.max_chars.map_or(usize::MAX, |max| {
        let staying = text.chars().count() - text[replacing].chars().count();
        max.saturating_sub(staying)
    });
    typed
        .chars()
        // A single line holds no line breaks; a pasted one becomes a space.
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .filter(|&c| !c.is_control() && rules.accept.is_none_or(|accept| accept(c)))
        .take(left)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Space,
    Word,
    Punctuation,
}

fn class_of(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
}

/// Where `step` in `direction` from `at` lands.
fn target(text: &str, at: usize, step: Step, direction: Direction) -> usize {
    match (step, direction) {
        (Step::Char, Direction::Back) => {
            text[..at].char_indices().next_back().map_or(0, |(i, _)| i)
        }
        (Step::Char, Direction::Forward) => {
            text[at..].chars().next().map_or(at, |c| at + c.len_utf8())
        }
        (Step::Word, Direction::Back) => word_back(text, at),
        (Step::Word, Direction::Forward) => word_forward(text, at),
        (Step::Line, Direction::Back) => 0,
        (Step::Line, Direction::Forward) => text.len(),
    }
}

/// The start of the word before `at`, past any spaces.
fn word_back(text: &str, at: usize) -> usize {
    let mut chars = text[..at]
        .char_indices()
        .rev()
        .skip_while(|&(_, c)| c.is_whitespace());
    let Some((mut start, first)) = chars.next() else {
        return 0;
    };
    let class = class_of(first);
    for (i, c) in chars {
        if class_of(c) != class {
            break;
        }
        start = i;
    }
    start
}

/// The end of the word after `at`, and the spaces after it.
fn word_forward(text: &str, at: usize) -> usize {
    let rest = &text[at..];
    let Some(first) = rest.chars().next() else {
        return at;
    };
    let class = class_of(first);
    let word_end = rest
        .char_indices()
        .find(|&(_, c)| class_of(c) != class)
        .map_or(rest.len(), |(i, _)| i);
    let spaces = rest[word_end..]
        .char_indices()
        .find(|&(_, c)| !c.is_whitespace())
        .map_or(rest.len() - word_end, |(i, _)| i);
    at + word_end + if class == Class::Space { 0 } else { spaces }
}

/// The nearest character boundary at or before `at`, inside `text`.
fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editing(text: &str) -> (String, TextEdit) {
        let mut edit = TextEdit::new();
        edit.set_caret(text, text.len(), false);
        (text.to_string(), edit)
    }

    #[test]
    fn typing_goes_in_at_the_caret() {
        let (mut text, mut edit) = editing("held");
        edit.set_caret(&text, 2, false);
        edit.insert(&mut text, "ll-ho", Rules::default());
        assert_eq!(text, "hell-hold");
        assert_eq!(edit.caret(), 7);
    }

    #[test]
    fn typing_replaces_the_selection() {
        let (mut text, mut edit) = editing("old text");
        edit.select_all(&text);
        edit.insert(&mut text, "new", Rules::default());
        assert_eq!(text, "new");
        assert!(!edit.has_selection());
    }

    #[test]
    fn motion_steps_over_whole_characters() {
        let (text, mut edit) = editing("가a😀");
        edit.move_caret(&text, Step::Char, Direction::Back, false);
        assert_eq!(edit.caret(), "가a".len());
        edit.move_caret(&text, Step::Char, Direction::Back, false);
        edit.move_caret(&text, Step::Char, Direction::Back, false);
        assert_eq!(edit.caret(), 0);
        edit.move_caret(&text, Step::Char, Direction::Back, false);
        assert_eq!(edit.caret(), 0);
    }

    #[test]
    fn word_motion_skips_spaces_and_stops_at_punctuation() {
        let text = "hello,  big world";
        let mut edit = TextEdit::new();
        edit.set_caret(text, text.len(), false);
        edit.move_caret(text, Step::Word, Direction::Back, false);
        assert_eq!(&text[edit.caret()..], "world");
        edit.move_caret(text, Step::Word, Direction::Back, false);
        assert_eq!(&text[edit.caret()..], "big world");
        edit.move_caret(text, Step::Word, Direction::Back, false);
        assert_eq!(&text[edit.caret()..], ",  big world");
        edit.set_caret(text, 0, false);
        edit.move_caret(text, Step::Word, Direction::Forward, false);
        assert_eq!(&text[edit.caret()..], ",  big world");
        edit.move_caret(text, Step::Word, Direction::Forward, false);
        assert_eq!(&text[edit.caret()..], "big world");
    }

    #[test]
    fn a_plain_arrow_collapses_the_selection_to_its_side() {
        let (text, mut edit) = editing("abcdef");
        edit.set_caret(&text, 1, false);
        edit.set_caret(&text, 4, true);
        edit.move_caret(&text, Step::Char, Direction::Back, false);
        assert_eq!((edit.caret(), edit.has_selection()), (1, false));
    }

    #[test]
    fn shift_extends_and_the_anchor_stays() {
        let (text, mut edit) = editing("abcdef");
        edit.set_caret(&text, 2, false);
        edit.move_caret(&text, Step::Line, Direction::Forward, true);
        assert_eq!(edit.selected(&text), "cdef");
        edit.move_caret(&text, Step::Word, Direction::Back, true);
        assert_eq!(edit.selected(&text), "ab");
    }

    #[test]
    fn deleting_words_and_characters() {
        let (mut text, mut edit) = editing("one two three");
        edit.delete(&mut text, Step::Word, Direction::Back);
        assert_eq!(text, "one two ");
        edit.delete(&mut text, Step::Char, Direction::Back);
        assert_eq!(text, "one two");
        edit.set_caret(&text, 0, false);
        edit.delete(&mut text, Step::Word, Direction::Forward);
        assert_eq!(text, "two");
        assert!(!edit.delete(&mut text, Step::Char, Direction::Back));
    }

    #[test]
    fn rules_cut_a_paste_down_to_what_fits() {
        let (mut text, mut edit) = editing("12");
        let digits = |c: char| c.is_ascii_digit();
        let rules = Rules {
            max_chars: Some(5),
            accept: Some(&digits),
        };
        edit.insert(&mut text, "3a4b5678", rules);
        assert_eq!(text, "12345");
        assert!(!edit.insert(&mut text, "9", rules));
    }

    #[test]
    fn a_pasted_line_break_becomes_a_space() {
        let (mut text, mut edit) = editing("");
        edit.insert(&mut text, "a\r\nb\tc", Rules::default());
        assert_eq!(text, "a  bc");
    }

    #[test]
    fn a_composition_replaces_the_selection_and_stays_out_of_the_text() {
        let (mut text, mut edit) = editing("old");
        edit.select_all(&text);
        edit.set_preedit(&mut text, "ㅎ", Some(3));
        assert_eq!((text.as_str(), edit.preedit()), ("", "ㅎ"));
        edit.insert(&mut text, "한", Rules::default());
        edit.set_preedit(&mut text, "", None);
        assert_eq!((text.as_str(), edit.preedit()), ("한", ""));
    }

    #[test]
    fn a_typed_word_undoes_as_one_step() {
        let (mut text, mut edit) = editing("");
        for c in ["h", "i", " ", "y", "o"] {
            edit.insert(&mut text, c, Rules::default());
        }
        assert_eq!(text, "hi yo");
        edit.undo(&mut text);
        assert_eq!(text, "hi ");
        edit.undo(&mut text);
        assert_eq!(text, "hi");
        edit.undo(&mut text);
        assert_eq!(text, "");
        edit.redo(&mut text);
        assert_eq!(text, "hi");
        assert_eq!(edit.caret(), 2);
    }

    #[test]
    fn a_new_edit_after_undo_drops_the_redo() {
        let (mut text, mut edit) = editing("");
        edit.insert(&mut text, "a", Rules::default());
        edit.undo(&mut text);
        edit.insert(&mut text, "b", Rules::default());
        assert!(!edit.redo(&mut text));
        assert_eq!(text, "b");
    }

    #[test]
    fn history_is_bounded() {
        let (mut text, mut edit) = editing("");
        for _ in 0..HISTORY * 2 {
            edit.insert(&mut text, " ", Rules::default());
        }
        let mut steps = 0;
        while edit.undo(&mut text) {
            steps += 1;
        }
        assert_eq!(steps, HISTORY);
    }

    #[test]
    fn double_click_picks_the_word() {
        let (text, mut edit) = editing("call me, ishmael");
        edit.select_word_at(&text, 10);
        assert_eq!(edit.selected(&text), "ishmael");
        edit.select_word_at(&text, 7);
        assert_eq!(edit.selected(&text), ",");
    }

    #[test]
    fn clamp_mends_positions_after_the_text_shrank() {
        let (mut text, mut edit) = editing("가나다");
        text.truncate(3);
        text.push('x');
        edit.clamp(&text);
        assert!(text.is_char_boundary(edit.caret()));
        assert!(edit.caret() <= text.len());
    }

    #[test]
    fn cut_takes_the_selection() {
        let (mut text, mut edit) = editing("keep cut");
        edit.set_caret(&text, 4, false);
        edit.set_caret(&text, 8, true);
        assert_eq!(edit.cut(&mut text).as_deref(), Some(" cut"));
        assert_eq!(text, "keep");
        assert_eq!(edit.cut(&mut text), None);
    }
}
