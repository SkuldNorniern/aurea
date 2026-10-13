//! Window event types for external event loop integration.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum WindowEvent {
    CloseRequested,
    /// The window is closing because the app called `Window::close`.
    Closed,
    Resized {
        width: u32,
        height: u32,
    },
    Moved {
        x: i32,
        y: i32,
    },
    Focused,
    Unfocused,
    Minimized,
    Restored,
    ScaleFactorChanged {
        scale_factor: f32,
    },
    SurfaceLost,
    SurfaceRecreated,
    /// The pointer came into the window.
    MouseEntered,
    /// The pointer left the window.
    MouseExited,
    /// Pointer position in logical pixels, relative to the window's content (below any title bar or
    /// menu bar).
    MouseMove {
        x: f64,
        y: f64,
        buttons: MouseButtons,
        modifiers: Modifiers,
        time: Duration,
    },
    RawMouseMotion {
        delta_x: f64,
        delta_y: f64,
    },
    /// A button went down or up, at the same kind of position as [`WindowEvent::MouseMove`].
    MouseButton {
        button: MouseButton,
        pressed: bool,
        modifiers: Modifiers,
        x: f64,
        y: f64,
        click_count: u8,
        time: Duration,
    },
    /// The wheel or a touchpad scrolled.
    MouseWheel {
        delta_x: f64,
        delta_y: f64,
        modifiers: Modifiers,
    },
    KeyInput {
        key: KeyCode,
        pressed: bool,
        modifiers: Modifiers,
    },
    /// Files dragged over the window, at a position in its content.
    FilesHovered {
        paths: Vec<PathBuf>,
        x: f64,
        y: f64,
    },
    /// The drag of [`WindowEvent::FilesHovered`] left or was called off.
    FilesHoverEnded,
    /// Files dropped on the window, at a position in its content.
    FilesDropped {
        paths: Vec<PathBuf>,
        x: f64,
        y: f64,
    },
    /// Text typed into the window, to insert.
    TextInput {
        text: String,
    },
    /// Text an input method is still composing, such as a Hangul syllable being built or a kana run
    /// before conversion.
    ImePreedit {
        text: String,
        cursor: Option<usize>,
    },
    /// A pen on or over a tablet or a pen display.
    Pen(PenSample),
}

/// One reading of a pen.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PenSample {
    /// Position in logical pixels, like [`WindowEvent::MouseMove`], but not rounded to whole pixels
    /// where the tablet reports finer.
    pub x: f64,
    pub y: f64,
    /// How hard the pen presses, from 0.0 to 1.0. 0.0 while hovering; 1.0 on touch for a pen that
    /// does not report pressure.
    pub pressure: f32,
    /// Lean in degrees, -90 to 90: positive `tilt_x` toward the right, positive `tilt_y` toward the
    /// user.
    pub tilt_x: f32,
    pub tilt_y: f32,
    /// Rotation about the pen's own axis in degrees, 0 to 360.
    pub twist: f32,
    pub phase: PenPhase,
    pub tool: PenTool,
    /// The barrel button is held.
    pub barrel: bool,
    pub modifiers: Modifiers,
    /// When the pen was read, as time since a fixed point early in the process, on the host's
    /// monotonic clock.
    pub time: Duration,
}

/// Where a [`PenSample`] falls in a stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PenPhase {
    /// In range but not touching.
    #[default]
    Hover,
    /// Touched down; a stroke starts.
    Down,
    /// Moving while touching.
    Move,
    /// Lifted; the stroke ends.
    Up,
}

/// Which end of the pen is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PenTool {
    #[default]
    Tip,
    /// The eraser end, or the tip turned to erase.
    Eraser,
}

impl PenPhase {
    /// The number the platform layer hands over for this phase.
    pub fn from_raw(phase: u32) -> Self {
        match phase {
            1 => Self::Down,
            2 => Self::Move,
            3 => Self::Up,
            _ => Self::Hover,
        }
    }

    /// Whether the pen touches the surface.
    pub fn touching(self) -> bool {
        matches!(self, Self::Down | Self::Move)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

impl MouseButton {
    pub fn from_raw(button: u8) -> Self {
        match button {
            0 => Self::Left,
            1 => Self::Right,
            2 => Self::Middle,
            other => Self::Other(other),
        }
    }
}

impl MouseButton {
    /// The number [`MouseButton::from_raw`] reads this button from.
    pub fn to_raw(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Middle => 2,
            Self::Other(n) => n,
        }
    }
}

/// The mouse buttons held at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MouseButtons(u8);

impl MouseButtons {
    pub fn from_bits(bits: u32) -> Self {
        Self(u8::try_from(bits & 0xFF).unwrap_or(0))
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn contains(self, button: MouseButton) -> bool {
        let n = button.to_raw();
        n < 8 && self.0 & (1 << n) != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_any(&self) -> bool {
        self.shift || self.ctrl || self.alt || self.meta
    }

    /// Whether the platform's shortcut key is held: Command on macOS and iOS, where it arrives as
    /// [`Modifiers::meta`], and Ctrl elsewhere.
    pub fn command(&self) -> bool {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            self.meta
        } else {
            self.ctrl
        }
    }

    /// The bits [`Modifiers::from_bits`] reads.
    pub fn bits(self) -> u32 {
        u32::from(self.shift)
            | u32::from(self.ctrl) << 1
            | u32::from(self.alt) << 2
            | u32::from(self.meta) << 3
    }

    pub fn from_bits(bits: u32) -> Self {
        Self {
            shift: bits & 0b0001 != 0,
            ctrl: bits & 0b0010 != 0,
            alt: bits & 0b0100 != 0,
            meta: bits & 0b1000 != 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Shift,
    Control,
    Alt,
    Meta,
    /// Punctuation / OEM keys, named by their unshifted US-layout character.
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Grave,
    Comma,
    Period,
    Slash,
    Unknown(u32),
}

impl KeyCode {
    /// The code [`KeyCode::from_raw`] reads this key from; `None` for [`KeyCode::Unknown`].
    pub fn to_raw(self) -> Option<u32> {
        if matches!(self, Self::Unknown(_)) {
            return None;
        }
        (0..=77).find(|&code| Self::from_raw(code) == self)
    }

    pub fn from_raw(code: u32) -> Self {
        match code {
            0..=25 => [
                Self::A,
                Self::B,
                Self::C,
                Self::D,
                Self::E,
                Self::F,
                Self::G,
                Self::H,
                Self::I,
                Self::J,
                Self::K,
                Self::L,
                Self::M,
                Self::N,
                Self::O,
                Self::P,
                Self::Q,
                Self::R,
                Self::S,
                Self::T,
                Self::U,
                Self::V,
                Self::W,
                Self::X,
                Self::Y,
                Self::Z,
            ][code as usize],
            26..=35 => [
                Self::Key0,
                Self::Key1,
                Self::Key2,
                Self::Key3,
                Self::Key4,
                Self::Key5,
                Self::Key6,
                Self::Key7,
                Self::Key8,
                Self::Key9,
            ][(code - 26) as usize],
            36 => Self::Space,
            37 => Self::Enter,
            38 => Self::Escape,
            39 => Self::Tab,
            40 => Self::Backspace,
            41 => Self::Delete,
            42 => Self::Insert,
            43 => Self::Home,
            44 => Self::End,
            45 => Self::PageUp,
            46 => Self::PageDown,
            47 => Self::Up,
            48 => Self::Down,
            49 => Self::Left,
            50 => Self::Right,
            51..=62 => [
                Self::F1,
                Self::F2,
                Self::F3,
                Self::F4,
                Self::F5,
                Self::F6,
                Self::F7,
                Self::F8,
                Self::F9,
                Self::F10,
                Self::F11,
                Self::F12,
            ][(code - 51) as usize],
            63 => Self::Shift,
            64 => Self::Control,
            65 => Self::Alt,
            66 => Self::Meta,
            67 => Self::Minus,
            68 => Self::Equals,
            69 => Self::LeftBracket,
            70 => Self::RightBracket,
            71 => Self::Backslash,
            72 => Self::Semicolon,
            73 => Self::Apostrophe,
            74 => Self::Grave,
            75 => Self::Comma,
            76 => Self::Period,
            77 => Self::Slash,
            _ => Self::Unknown(code),
        }
    }
}

/// An application's window-event handler.
pub type EventCallback = Rc<dyn Fn(WindowEvent)>;

#[cfg(test)]
mod keycode_tests {
    use super::{KeyCode, Modifiers};

    #[test]
    fn command_is_the_platform_shortcut_key() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let meta = Modifiers {
            meta: true,
            ..Modifiers::default()
        };
        let apple = cfg!(any(target_os = "macos", target_os = "ios"));
        assert_eq!(ctrl.command(), !apple);
        assert_eq!(meta.command(), apple);
        assert!(!Modifiers::default().command());
    }

    #[test]
    fn from_raw_maps_letters_digits_and_modifiers() {
        assert_eq!(KeyCode::from_raw(0), KeyCode::A);
        assert_eq!(KeyCode::from_raw(25), KeyCode::Z);
        assert_eq!(KeyCode::from_raw(26), KeyCode::Key0);
        assert_eq!(KeyCode::from_raw(66), KeyCode::Meta);
    }

    #[test]
    fn raw_codes_round_trip() {
        for code in 0..=77 {
            assert_eq!(KeyCode::from_raw(code).to_raw(), Some(code));
        }
        assert_eq!(KeyCode::Unknown(500).to_raw(), None);
        for bits in 0..16 {
            assert_eq!(Modifiers::from_bits(bits).bits(), bits);
        }
    }

    #[test]
    fn from_raw_maps_punctuation() {
        assert_eq!(KeyCode::from_raw(67), KeyCode::Minus);
        assert_eq!(KeyCode::from_raw(68), KeyCode::Equals);
        assert_eq!(KeyCode::from_raw(77), KeyCode::Slash);
        assert!(matches!(
            KeyCode::from_raw(0xFFFF_FFFF),
            KeyCode::Unknown(_)
        ));
    }
}
