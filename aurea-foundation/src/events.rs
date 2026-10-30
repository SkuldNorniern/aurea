//! Window event types for external event loop integration.

use std::path::PathBuf;
use std::rc::Rc;
use std::slice;
use std::time::Duration;

/// A window event delivered to the app.
#[derive(Debug, Clone)]
pub enum WindowEvent {
    /// The user asked to close the window.
    CloseRequested,
    /// The window is closing because the app called `Window::close`.
    Closed,
    /// The window's content size changed.
    Resized {
        /// Width in logical pixels.
        width: u32,
        /// Height in logical pixels.
        height: u32,
    },
    /// The window moved on screen.
    Moved {
        /// Left edge on screen.
        x: i32,
        /// Top edge on screen.
        y: i32,
    },
    /// The window gained keyboard focus.
    Focused,
    /// The window lost keyboard focus.
    Unfocused,
    /// The window was minimized.
    Minimized,
    /// The window was restored from minimized.
    Restored,
    /// Where the pen will likely be next, ahead of its real samples, on systems that predict it
    /// (iOS). Draw it as a provisional tail of the stroke and drop it at the next [`Self::Pen`]:
    /// the stroke then seems to keep up with the pen.
    PenPredicted(PenSample),
    /// A pen's own gesture: a double tap on an Apple Pencil, or a squeeze of a Pencil Pro.
    PenTap {
        /// A squeeze rather than a tap.
        squeeze: bool,
        /// What the user set this gesture to do in the system's settings, for the app to follow.
        action: PenTapAction,
    },
    /// The system is short of memory: drop caches that can be rebuilt. On iOS an app that does not
    /// may be stopped.
    MemoryWarning,
    /// The display scale factor changed.
    ScaleFactorChanged {
        /// New scale factor.
        scale_factor: f32,
    },
    /// The rendering surface was lost.
    SurfaceLost,
    /// The rendering surface was recreated.
    SurfaceRecreated,
    /// The pointer came into the window.
    MouseEntered,
    /// The pointer left the window.
    MouseExited,
    /// Pointer position in logical pixels, relative to the window's content (below any title bar or
    /// menu bar).
    MouseMove {
        /// Horizontal pointer position.
        x: f64,
        /// Vertical pointer position.
        y: f64,
        /// Buttons held down.
        buttons: MouseButtons,
        /// Modifier keys held down.
        modifiers: Modifiers,
        /// What moved the pointer.
        pointer: PointerKind,
        /// When the event happened.
        time: Duration,
    },
    /// Unaccelerated mouse movement.
    RawMouseMotion {
        /// Horizontal movement.
        delta_x: f64,
        /// Vertical movement.
        delta_y: f64,
    },
    /// A button went down or up, at the same kind of position as [`WindowEvent::MouseMove`].
    MouseButton {
        /// The button that changed.
        button: MouseButton,
        /// True when the button went down.
        pressed: bool,
        /// Modifier keys held down.
        modifiers: Modifiers,
        /// Horizontal pointer position.
        x: f64,
        /// Vertical pointer position.
        y: f64,
        /// Clicks in quick succession, including this one.
        click_count: u8,
        /// What pressed the button: a pen's contact is sent as [`WindowEvent::Pen`] too, so a
        /// drawing can drop this copy of it.
        pointer: PointerKind,
        /// When the event happened.
        time: Duration,
    },
    /// The wheel or a touchpad scrolled: in notches on Windows and Linux (fractions from touchpads
    /// and fine wheels), in points on macOS.
    MouseWheel {
        /// Horizontal scroll amount.
        delta_x: f64,
        /// Vertical scroll amount.
        delta_y: f64,
        /// Modifier keys held down.
        modifiers: Modifiers,
    },
    /// A key was pressed or released.
    KeyInput {
        /// The key that changed.
        key: KeyCode,
        /// True when the key went down.
        pressed: bool,
        /// Modifier keys held down.
        modifiers: Modifiers,
    },
    /// Files dragged over the window, at a position in its content.
    FilesHovered {
        /// Paths of the dragged files.
        paths: Vec<PathBuf>,
        /// Horizontal pointer position.
        x: f64,
        /// Vertical pointer position.
        y: f64,
    },
    /// The drag of [`WindowEvent::FilesHovered`] left or was called off.
    FilesHoverEnded,
    /// Files dropped on the window, at a position in its content.
    FilesDropped {
        /// Paths of the dropped files.
        paths: Vec<PathBuf>,
        /// Horizontal pointer position.
        x: f64,
        /// Vertical pointer position.
        y: f64,
    },
    /// Text typed into the window, to insert.
    TextInput {
        /// Text to insert.
        text: String,
    },
    /// Text an input method is still composing, such as a Hangul syllable being built or a kana run
    /// before conversion.
    ImePreedit {
        /// Text still being composed.
        text: String,
        /// Cursor position within `text`, if shown.
        cursor: Option<usize>,
    },
    /// A pen on or over a tablet or a pen display. Every host also sends the pen's contact as
    /// mouse events, and those are what widgets react to; this carries what a drawing needs on top
    /// (pressure, tilt, every sample between frames), so a `UiState` fed only `Pen` events never
    /// hovers or presses anything.
    Pen(PenSample),
    /// Consecutive actual pen readings, oldest first, when desktop pen batching is
    /// enabled. Samples keep their individual phase, pressure, modifiers and time.
    /// Other input events form batch boundaries; predicted readings stay separate.
    PenBatch(Vec<PenSample>),
    /// A pinch: the content should grow by `factor` about (`x`, `y`), in logical pixels. Two
    /// fingers moving together come as [`WindowEvent::MouseWheel`] instead.
    Magnify {
        /// How much bigger since the last one: above 1 grows, below 1 shrinks.
        factor: f64,
        /// Where the fingers meet, horizontally.
        x: f64,
        /// Where the fingers meet, vertically.
        y: f64,
    },
    /// A twist: the content should turn by `radians` about (`x`, `y`), in logical pixels. Positive
    /// turns clockwise on screen, as `PanZoom` rotation does.
    Rotate {
        /// How far it turned since the last one, in radians.
        radians: f64,
        /// Where the fingers meet, horizontally.
        x: f64,
        /// Where the fingers meet, vertically.
        y: f64,
    },
    /// Other windows now cover more or less of this one, or it was minimized or restored; see
    /// [`Occlusion`]. An app can pause work no one sees, or warn when its window is hidden.
    OcclusionChanged(Occlusion),
    /// The system switched between light and dark appearance. Sent to every window; see
    /// `aurea::prefers_dark` for the current setting.
    ThemeChanged {
        /// True when the system now prefers dark.
        dark: bool,
    },
}

/// How much of a window shows on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Occlusion {
    /// All of it shows.
    #[default]
    Visible,
    /// Other windows cover part of it. Windows only; elsewhere a window reads as visible or covered.
    PartlyCovered,
    /// None of it shows: covered, minimized, off screen, or on another virtual desktop.
    Covered,
}

impl Occlusion {
    /// The level the platform layer hands over: 0 visible, 1 partly covered, 2 covered.
    pub fn from_raw(level: i32) -> Self {
        match level {
            1 => Self::PartlyCovered,
            2 => Self::Covered,
            _ => Self::Visible,
        }
    }
}

impl WindowEvent {
    /// Actual pen readings carried by this event, for code accepting both single
    /// readings and batches. Returns an empty slice for other events, including
    /// predictions, which must be handled separately.
    pub fn pen_samples(&self) -> &[PenSample] {
        match self {
            Self::Pen(sample) => slice::from_ref(sample),
            Self::PenBatch(samples) => samples,
            _ => &[],
        }
    }
}

/// One reading of a pen.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PenSample {
    /// Position in logical pixels, like [`WindowEvent::MouseMove`], but not rounded to whole pixels
    /// where the tablet reports finer.
    pub x: f64,
    /// Vertical position, as for `x`.
    pub y: f64,
    /// How hard the pen presses, from 0.0 to 1.0. 0.0 while hovering; 1.0 on touch for a pen that
    /// does not report pressure.
    pub pressure: f32,
    /// Lean in degrees, -90 to 90: positive `tilt_x` toward the right, positive `tilt_y` toward the
    /// user.
    pub tilt_x: f32,
    /// Lean in degrees, as for `tilt_x`.
    pub tilt_y: f32,
    /// Rotation about the pen's own axis in degrees, 0 to 360.
    pub twist: f32,
    /// Where the pen is in its stroke.
    pub phase: PenPhase,
    /// Which end of the pen is in use.
    pub tool: PenTool,
    /// The barrel button is held.
    pub barrel: bool,
    /// Modifier keys held down.
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

/// What the user set a pen's tap or squeeze to do; see [`WindowEvent::PenTap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PenTapAction {
    /// Nothing: the app should ignore it.
    Ignore,
    /// Switch between the current tool and the eraser.
    #[default]
    SwitchEraser,
    /// Switch between the current tool and the one before.
    SwitchPrevious,
    /// Show the colour or tool palette.
    ShowPalette,
    /// Something the system does itself, such as a shortcut.
    System,
}

impl PenTapAction {
    /// The number the platform layer hands over for this action.
    pub fn from_raw(action: u32) -> Self {
        match action {
            0 => Self::Ignore,
            1 => Self::SwitchEraser,
            2 => Self::SwitchPrevious,
            3 => Self::ShowPalette,
            _ => Self::System,
        }
    }
}

/// Which end of the pen is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PenTool {
    /// The writing tip.
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

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// The left button.
    Left,
    /// The right button.
    Right,
    /// The middle button.
    Middle,
    /// Any other button, by its raw number.
    Other(u8),
}

/// What moves the pointer behind a mouse event. Hosts send a pen's contact and a finger's as mouse
/// events too, so widgets work with either; this tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PointerKind {
    /// A mouse or a touchpad.
    #[default]
    Mouse,
    /// A finger on a touch screen.
    Touch,
    /// A pen, or its eraser.
    Pen,
}

impl PointerKind {
    /// Reads the kind from a native mouse event's modifier bits, where it rides above the keys.
    pub fn from_bits(bits: u32) -> Self {
        if bits & 0x1_0000 != 0 {
            Self::Pen
        } else if bits & 0x2_0000 != 0 {
            Self::Touch
        } else {
            Self::Mouse
        }
    }
}

impl MouseButton {
    /// Reads a button from its raw number.
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
    /// Reads the buttons from a bit set.
    pub fn from_bits(bits: u32) -> Self {
        Self(u8::try_from(bits & 0xFF).unwrap_or(0))
    }

    /// The buttons as a bit set.
    pub fn bits(self) -> u8 {
        self.0
    }

    /// Whether no buttons are held.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether `button` is held.
    pub fn contains(self, button: MouseButton) -> bool {
        let n = button.to_raw();
        n < 8 && self.0 & (1 << n) != 0
    }
}

/// Modifier keys held at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    /// Shift is held.
    pub shift: bool,
    /// Ctrl is held.
    pub ctrl: bool,
    /// Alt is held.
    pub alt: bool,
    /// Meta (Windows key or Command) is held.
    pub meta: bool,
}

impl Modifiers {
    /// Modifiers with no keys held.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether any modifier is held.
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

    /// Reads modifiers from a bit set.
    pub fn from_bits(bits: u32) -> Self {
        Self {
            shift: bits & 0b0001 != 0,
            ctrl: bits & 0b0010 != 0,
            alt: bits & 0b0100 != 0,
            meta: bits & 0b1000 != 0,
        }
    }
}

/// A keyboard key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// The A key.
    A,
    /// The B key.
    B,
    /// The C key.
    C,
    /// The D key.
    D,
    /// The E key.
    E,
    /// The F key.
    F,
    /// The G key.
    G,
    /// The H key.
    H,
    /// The I key.
    I,
    /// The J key.
    J,
    /// The K key.
    K,
    /// The L key.
    L,
    /// The M key.
    M,
    /// The N key.
    N,
    /// The O key.
    O,
    /// The P key.
    P,
    /// The Q key.
    Q,
    /// The R key.
    R,
    /// The S key.
    S,
    /// The T key.
    T,
    /// The U key.
    U,
    /// The V key.
    V,
    /// The W key.
    W,
    /// The X key.
    X,
    /// The Y key.
    Y,
    /// The Z key.
    Z,
    /// The 0 key.
    Key0,
    /// The 1 key.
    Key1,
    /// The 2 key.
    Key2,
    /// The 3 key.
    Key3,
    /// The 4 key.
    Key4,
    /// The 5 key.
    Key5,
    /// The 6 key.
    Key6,
    /// The 7 key.
    Key7,
    /// The 8 key.
    Key8,
    /// The 9 key.
    Key9,
    /// The space bar.
    Space,
    /// The Enter or Return key.
    Enter,
    /// The Escape key.
    Escape,
    /// The Tab key.
    Tab,
    /// The Backspace key.
    Backspace,
    /// The Delete key.
    Delete,
    /// The Insert key.
    Insert,
    /// The Home key.
    Home,
    /// The End key.
    End,
    /// The Page Up key.
    PageUp,
    /// The Page Down key.
    PageDown,
    /// The up arrow key.
    Up,
    /// The down arrow key.
    Down,
    /// The left arrow key.
    Left,
    /// The right arrow key.
    Right,
    /// The F1 key.
    F1,
    /// The F2 key.
    F2,
    /// The F3 key.
    F3,
    /// The F4 key.
    F4,
    /// The F5 key.
    F5,
    /// The F6 key.
    F6,
    /// The F7 key.
    F7,
    /// The F8 key.
    F8,
    /// The F9 key.
    F9,
    /// The F10 key.
    F10,
    /// The F11 key.
    F11,
    /// The F12 key.
    F12,
    /// The Shift key.
    Shift,
    /// The Control key.
    Control,
    /// The Alt key.
    Alt,
    /// The Meta key.
    Meta,
    /// Punctuation / OEM keys, named by their unshifted US-layout character.
    Minus,
    /// The = key.
    Equals,
    /// The [ key.
    LeftBracket,
    /// The ] key.
    RightBracket,
    /// The \ key.
    Backslash,
    /// The ; key.
    Semicolon,
    /// The ' key.
    Apostrophe,
    /// The ` key.
    Grave,
    /// The , key.
    Comma,
    /// The . key.
    Period,
    /// The / key.
    Slash,
    /// A key this list does not name, by its raw code.
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

    /// Reads a key from its raw code.
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
pub type EventCallback = Rc<dyn Fn(&WindowEvent)>;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_kinds_ride_above_the_modifier_keys() {
        // NG_POINTER_PEN and NG_POINTER_TOUCH in common/input.h
        assert_eq!(PointerKind::from_bits(0x1_0000 | 0b0011), PointerKind::Pen);
        assert_eq!(PointerKind::from_bits(0x2_0000), PointerKind::Touch);
        assert_eq!(PointerKind::from_bits(0b1111), PointerKind::Mouse);
        let mods = Modifiers::from_bits(0x3_0001);
        assert!(mods.shift && !mods.ctrl && !mods.alt && !mods.meta);
    }
}
