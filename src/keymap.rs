//! App-wide keyboard shortcuts.

use std::mem;

use crate::AureaResult;
use crate::window::{KeyCode, Modifiers, Window, WindowEvent};

/// A key and the modifiers held with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    /// The key pressed.
    pub key: KeyCode,
    /// Modifiers held with the key.
    pub modifiers: Modifiers,
}

impl Chord {
    /// The key alone.
    pub const fn key(key: KeyCode) -> Self {
        Self {
            key,
            modifiers: Modifiers {
                shift: false,
                ctrl: false,
                alt: false,
                meta: false,
            },
        }
    }

    /// The key with the platform's shortcut key: Command on macOS and iOS, Ctrl elsewhere.
    pub const fn command(key: KeyCode) -> Self {
        let apple = cfg!(any(target_os = "macos", target_os = "ios"));
        let mut chord = Self::key(key);
        chord.modifiers.ctrl = !apple;
        chord.modifiers.meta = apple;
        chord
    }

    /// Adds Shift to the chord.
    #[must_use]
    pub const fn shift(mut self) -> Self {
        self.modifiers.shift = true;
        self
    }

    /// Adds Alt to the chord.
    #[must_use]
    pub const fn alt(mut self) -> Self {
        self.modifiers.alt = true;
        self
    }

    /// Adds Ctrl to the chord.
    #[must_use]
    pub const fn ctrl(mut self) -> Self {
        self.modifiers.ctrl = true;
        self
    }

    /// The chord a key press is, if `event` is one.
    pub fn of(event: &WindowEvent) -> Option<Self> {
        match *event {
            WindowEvent::KeyInput {
                key,
                pressed: true,
                modifiers,
            } => Some(Self { key, modifiers }),
            _ => None,
        }
    }

    /// Whether a focused text field uses this chord itself.
    pub fn edits_text(self) -> bool {
        let m = self.modifiers;
        let navigates = matches!(
            self.key,
            KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::Enter
                | KeyCode::Space
        );
        if navigates {
            return true;
        }
        if !(m.ctrl || m.alt || m.meta) {
            return !matches!(
                self.key,
                KeyCode::Escape
                    | KeyCode::Tab
                    | KeyCode::F1
                    | KeyCode::F2
                    | KeyCode::F3
                    | KeyCode::F4
                    | KeyCode::F5
                    | KeyCode::F6
                    | KeyCode::F7
                    | KeyCode::F8
                    | KeyCode::F9
                    | KeyCode::F10
                    | KeyCode::F11
                    | KeyCode::F12
            );
        }
        m.command()
            && !m.alt
            && matches!(
                self.key,
                KeyCode::A | KeyCode::C | KeyCode::V | KeyCode::X | KeyCode::Y | KeyCode::Z
            )
    }

    /// How the platform writes the chord: `⇧⌘S` on Apple systems, `Ctrl+Shift+S` elsewhere.
    pub fn label(self) -> String {
        let m = self.modifiers;
        let mut out = String::new();
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            for (held, sign) in [(m.ctrl, '⌃'), (m.alt, '⌥'), (m.shift, '⇧'), (m.meta, '⌘')]
            {
                if held {
                    out.push(sign);
                }
            }
        } else {
            let meta = if cfg!(windows) { "Win" } else { "Super" };
            for (held, name) in [
                (m.ctrl, "Ctrl"),
                (m.alt, "Alt"),
                (m.shift, "Shift"),
                (m.meta, meta),
            ] {
                if held {
                    out.push_str(name);
                    out.push('+');
                }
            }
        }
        out.push_str(key_name(self.key));
        out
    }
}

fn key_name(key: KeyCode) -> &'static str {
    const LETTERS: [&str; 26] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R",
        "S", "T", "U", "V", "W", "X", "Y", "Z",
    ];
    let apple = cfg!(any(target_os = "macos", target_os = "ios"));
    match key {
        KeyCode::Key0 => "0",
        KeyCode::Key1 => "1",
        KeyCode::Key2 => "2",
        KeyCode::Key3 => "3",
        KeyCode::Key4 => "4",
        KeyCode::Key5 => "5",
        KeyCode::Key6 => "6",
        KeyCode::Key7 => "7",
        KeyCode::Key8 => "8",
        KeyCode::Key9 => "9",
        KeyCode::Space => "Space",
        KeyCode::Enter if apple => "↩",
        KeyCode::Enter => "Enter",
        KeyCode::Escape if apple => "⎋",
        KeyCode::Escape => "Esc",
        KeyCode::Tab => "Tab",
        KeyCode::Backspace if apple => "⌫",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete if apple => "⌦",
        KeyCode::Delete => "Del",
        KeyCode::Insert => "Ins",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::PageUp => "PgUp",
        KeyCode::PageDown => "PgDn",
        KeyCode::Up => "↑",
        KeyCode::Down => "↓",
        KeyCode::Left => "←",
        KeyCode::Right => "→",
        KeyCode::F1 => "F1",
        KeyCode::F2 => "F2",
        KeyCode::F3 => "F3",
        KeyCode::F4 => "F4",
        KeyCode::F5 => "F5",
        KeyCode::F6 => "F6",
        KeyCode::F7 => "F7",
        KeyCode::F8 => "F8",
        KeyCode::F9 => "F9",
        KeyCode::F10 => "F10",
        KeyCode::F11 => "F11",
        KeyCode::F12 => "F12",
        KeyCode::Minus => "-",
        KeyCode::Equals => "=",
        KeyCode::LeftBracket => "[",
        KeyCode::RightBracket => "]",
        KeyCode::Backslash => "\\",
        KeyCode::Semicolon => ";",
        KeyCode::Apostrophe => "'",
        KeyCode::Grave => "`",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::Shift | KeyCode::Control | KeyCode::Alt | KeyCode::Meta | KeyCode::Unknown(_) => {
            ""
        }
        letter => letter_index(letter).map_or("", |i| LETTERS[i]),
    }
}

fn letter_index(key: KeyCode) -> Option<usize> {
    const ORDER: [KeyCode; 26] = [
        KeyCode::A,
        KeyCode::B,
        KeyCode::C,
        KeyCode::D,
        KeyCode::E,
        KeyCode::F,
        KeyCode::G,
        KeyCode::H,
        KeyCode::I,
        KeyCode::J,
        KeyCode::K,
        KeyCode::L,
        KeyCode::M,
        KeyCode::N,
        KeyCode::O,
        KeyCode::P,
        KeyCode::Q,
        KeyCode::R,
        KeyCode::S,
        KeyCode::T,
        KeyCode::U,
        KeyCode::V,
        KeyCode::W,
        KeyCode::X,
        KeyCode::Y,
        KeyCode::Z,
    ];
    ORDER.iter().position(|k| *k == key)
}

/// Chords bound to the app's actions.
///
/// On Windows a native menu bar takes Alt+letter for its mnemonics, so those chords may never
/// reach the app.
#[derive(Clone, Debug)]
pub struct Keymap<A> {
    bindings: Vec<(Chord, A)>,
}

impl<A> Default for Keymap<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A> Keymap<A> {
    /// Creates an empty keymap.
    pub const fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Binds `chord` to `action`, replacing what it was bound to; debug builds warn when it was
    /// bound already. [`Self::insert`] hands the old action back.
    pub fn bind(&mut self, chord: Chord, action: A) -> &mut Self {
        if self.insert(chord, action).is_some() && cfg!(debug_assertions) {
            log::warn!("{chord:?} was bound already; the new action replaces the old one");
        }
        self
    }

    /// Binds `chord` to `action`, returning what it was bound to before.
    pub fn insert(&mut self, chord: Chord, action: A) -> Option<A> {
        match self.bindings.iter_mut().find(|(c, _)| *c == chord) {
            Some(slot) => Some(mem::replace(&mut slot.1, action)),
            None => {
                self.bindings.push((chord, action));
                None
            }
        }
    }

    /// Drops what `chord` was bound to.
    pub fn unbind(&mut self, chord: Chord) {
        self.bindings.retain(|(c, _)| *c != chord);
    }

    /// The action a key press asks for.
    pub fn resolve(&self, event: &WindowEvent, typing: bool) -> Option<&A> {
        let chord = Chord::of(event)?;
        if typing && chord.edits_text() {
            return None;
        }
        self.action(chord)
    }

    /// What `chord` is bound to.
    pub fn action(&self, chord: Chord) -> Option<&A> {
        self.bindings
            .iter()
            .find(|(c, _)| *c == chord)
            .map(|(_, action)| action)
    }

    /// The chord bound to `action`, to show beside it.
    pub fn chord_for(&self, action: &A) -> Option<Chord>
    where
        A: PartialEq,
    {
        self.bindings
            .iter()
            .find(|(_, a)| a == action)
            .map(|(c, _)| *c)
    }

    /// Iterates over every binding.
    pub fn iter(&self) -> impl Iterator<Item = (Chord, &A)> {
        self.bindings.iter().map(|(c, a)| (*c, a))
    }

    /// Keeps the chords that are not text editing reaching `window` while a child view has the
    /// keyboard.
    pub fn install(&self, window: &Window) -> AureaResult<()> {
        let keys: Vec<(KeyCode, Modifiers)> = self
            .bindings
            .iter()
            .filter(|(c, _)| !c.edits_text())
            .map(|(c, _)| (c.key, c.modifiers))
            .collect();
        window.set_shortcuts(&keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Act {
        Save,
        Undo,
        Brush,
    }

    fn press(chord: Chord) -> WindowEvent {
        WindowEvent::KeyInput {
            key: chord.key,
            pressed: true,
            modifiers: chord.modifiers,
        }
    }

    fn keymap() -> Keymap<Act> {
        let mut map = Keymap::new();
        map.bind(Chord::command(KeyCode::S), Act::Save)
            .bind(Chord::command(KeyCode::Z), Act::Undo)
            .bind(Chord::key(KeyCode::B), Act::Brush);
        map
    }

    #[test]
    fn presses_resolve_to_their_action() {
        let map = keymap();
        assert_eq!(
            map.resolve(&press(Chord::key(KeyCode::B)), false),
            Some(&Act::Brush)
        );
        assert_eq!(
            map.resolve(&press(Chord::command(KeyCode::S)), false),
            Some(&Act::Save)
        );
        assert_eq!(
            map.resolve(&press(Chord::command(KeyCode::S).shift()), false),
            None
        );
        let release = WindowEvent::KeyInput {
            key: KeyCode::B,
            pressed: false,
            modifiers: Modifiers::default(),
        };
        assert_eq!(map.resolve(&release, false), None);
    }

    #[test]
    fn a_text_field_keeps_its_own_keys() {
        let map = keymap();
        assert_eq!(map.resolve(&press(Chord::key(KeyCode::B)), true), None);
        assert_eq!(map.resolve(&press(Chord::command(KeyCode::Z)), true), None);
        assert_eq!(
            map.resolve(&press(Chord::command(KeyCode::S)), true),
            Some(&Act::Save)
        );
        assert!(!Chord::key(KeyCode::F5).edits_text());
        assert!(Chord::key(KeyCode::Left).alt().edits_text());
    }

    #[test]
    fn binding_again_replaces() {
        let mut map = keymap();
        assert_eq!(
            map.insert(Chord::key(KeyCode::B), Act::Save),
            Some(Act::Brush)
        );
        assert_eq!(map.iter().count(), 3);
        assert_eq!(map.chord_for(&Act::Save), Some(Chord::command(KeyCode::S)));
        assert_eq!(map.chord_for(&Act::Brush), None);
        assert_eq!(map.insert(Chord::key(KeyCode::Q), Act::Brush), None);
        assert_eq!(map.chord_for(&Act::Brush), Some(Chord::key(KeyCode::Q)));
        map.unbind(Chord::command(KeyCode::S));
        assert_eq!(map.chord_for(&Act::Save), Some(Chord::key(KeyCode::B)));
        map.unbind(Chord::key(KeyCode::B));
        assert_eq!(map.chord_for(&Act::Save), None);
    }

    #[test]
    fn labels_follow_the_platform() {
        let label = Chord::command(KeyCode::S).shift().label();
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            assert_eq!(label, "⇧⌘S");
        } else {
            assert_eq!(label, "Ctrl+Shift+S");
        }
        assert_eq!(Chord::key(KeyCode::Slash).label(), "/");
    }
}
