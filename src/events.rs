//! Window event types for external event loop integration.
//!
//! Shared event types used by window event handling and runtime event queues.

use std::sync::Arc;

/// Window-level events for external event loop integration
#[derive(Debug, Clone)]
pub enum WindowEvent {
    CloseRequested,
    Resized { width: u32, height: u32 },
    Moved { x: i32, y: i32 },
    Focused,
    Unfocused,
    Minimized,
    Restored,
    ScaleFactorChanged { scale_factor: f32 },
    SurfaceLost,
    SurfaceRecreated,
    MouseEntered,
    MouseExited,
    MouseMove { x: f64, y: f64 },
    RawMouseMotion { delta_x: f64, delta_y: f64 },
    MouseButton {
        button: MouseButton,
        pressed: bool,
        modifiers: Modifiers,
    },
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
    TextInput { text: String },
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
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
    Space, Enter, Escape, Tab, Backspace, Delete, Insert, Home, End, PageUp, PageDown,
    Up, Down, Left, Right,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Shift, Control, Alt, Meta,
    Unknown(u32),
}

impl KeyCode {
    pub fn from_raw(code: u32) -> Self {
        match code {
            0..=25 => [
                Self::A, Self::B, Self::C, Self::D, Self::E, Self::F,
                Self::G, Self::H, Self::I, Self::J, Self::K, Self::L,
                Self::M, Self::N, Self::O, Self::P, Self::Q, Self::R,
                Self::S, Self::T, Self::U, Self::V, Self::W, Self::X,
                Self::Y, Self::Z,
            ][code as usize],
            26..=35 => [
                Self::Key0, Self::Key1, Self::Key2, Self::Key3, Self::Key4,
                Self::Key5, Self::Key6, Self::Key7, Self::Key8, Self::Key9,
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
                Self::F1, Self::F2, Self::F3, Self::F4, Self::F5, Self::F6,
                Self::F7, Self::F8, Self::F9, Self::F10, Self::F11, Self::F12,
            ][(code - 51) as usize],
            63 => Self::Shift,
            64 => Self::Control,
            65 => Self::Alt,
            66 => Self::Meta,
            _ => Self::Unknown(code),
        }
    }
}

pub type EventCallback = Arc<dyn Fn(WindowEvent) + Send + Sync>;
