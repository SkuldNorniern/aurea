//! Window event system for external event loop integration.

use std::sync::{Arc, Mutex};

/// Window-level events for external event loop integration.
#[derive(Debug, Clone)]
pub enum WindowEvent {
    /// Window close was requested by the user.
    CloseRequested,
    /// Window was resized.
    Resized {
        width: u32,
        height: u32,
    },
    /// Window was moved.
    Moved {
        x: i32,
        y: i32,
    },
    /// Window gained focus.
    Focused,
    /// Window lost focus.
    Unfocused,
    /// Mouse entered the window.
    MouseEntered,
    /// Mouse exited the window.
    MouseExited,
    /// Mouse moved within the window.
    MouseMove {
        x: f64,
        y: f64,
    },
    /// Mouse button was pressed or released.
    MouseButton {
        button: MouseButton,
        pressed: bool,
        modifiers: Modifiers,
    },
    /// Mouse wheel was scrolled.
    MouseWheel {
        delta_x: f64,
        delta_y: f64,
        modifiers: Modifiers,
    },
    /// Keyboard key was pressed or released.
    KeyInput {
        key: KeyCode,
        pressed: bool,
        modifiers: Modifiers,
    },
    /// Text input was received (IME, composition, etc.).
    TextInput {
        text: String,
    },
}

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

/// Keyboard modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    /// Create a new Modifiers struct.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if any modifier is pressed.
    pub fn is_any(&self) -> bool {
        self.shift || self.ctrl || self.alt || self.meta
    }
}

/// Keyboard key codes.
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

    Unknown(u32),
}

/// Event callback type.
pub type EventCallback = Arc<dyn Fn(WindowEvent) + Send + Sync>;

pub(crate) struct EventQueue {
    events: Mutex<Vec<WindowEvent>>,
    callbacks: Mutex<Vec<EventCallback>>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            callbacks: Mutex::new(Vec::new()),
        }
    }

    /// Push an event to the queue.
    pub fn push(&self, event: WindowEvent) {
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }

    /// Pop all events from the queue.
    pub fn pop_all(&self) -> Vec<WindowEvent> {
        let mut events = self.events.lock().unwrap();
        std::mem::take(&mut *events)
    }

    /// Register an event callback.
    pub fn register_callback(&self, callback: EventCallback) {
        let mut callbacks = self.callbacks.lock().unwrap();
        callbacks.push(callback);
    }

    /// Process all queued events by calling registered callbacks Returns the events that were
    /// processed (for manual processing if needed).
    pub fn process_events(&self) -> Vec<WindowEvent> {
        let events = self.pop_all();
        if events.is_empty() {
            return Vec::new();
        }

        let callbacks: Vec<EventCallback> = {
            let callbacks = self.callbacks.lock().unwrap();
            callbacks.clone()
        };

        for event in &events {
            for callback in &callbacks {
                callback(event.clone());
            }
        }

        events
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}
