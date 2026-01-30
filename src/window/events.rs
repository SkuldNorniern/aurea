//! Window event system for external event loop integration
//!
//! This module provides window-level events (resize, close, focus, input) for
//! integration with external event loops while maintaining Aurea's event-driven
//! invalidation model.

use std::sync::{Arc, Mutex};

/// Window-level events for external event loop integration
///
/// These events are emitted by the window and can be polled via `Window::poll_events()`
/// or handled via `Window::on_event()` callbacks.
#[derive(Debug, Clone)]
pub enum WindowEvent {
    /// Window close was requested by the user
    CloseRequested,
    /// Window was resized
    Resized { width: u32, height: u32 },
    /// Window was moved
    Moved { x: i32, y: i32 },
    /// Window gained focus
    Focused,
    /// Window lost focus
    Unfocused,
    /// Window was minimized
    Minimized,
    /// Window was restored from minimized state
    Restored,
    /// Window scale factor changed
    ScaleFactorChanged { scale_factor: f32 },
    /// Window surface was lost (context lost)
    SurfaceLost,
    /// Window surface was recreated (context restored)
    SurfaceRecreated,
    /// Mouse entered the window
    MouseEntered,
    /// Mouse exited the window
    MouseExited,
    /// Mouse moved within the window
    MouseMove { x: f64, y: f64 },
    /// Raw mouse motion (relative movement)
    RawMouseMotion { delta_x: f64, delta_y: f64 },
    /// Mouse button was pressed or released
    MouseButton {
        button: MouseButton,
        pressed: bool,
        modifiers: Modifiers,
    },
    /// Mouse wheel was scrolled
    MouseWheel {
        delta_x: f64,
        delta_y: f64,
        modifiers: Modifiers,
    },
    /// Keyboard key was pressed or released
    KeyInput {
        key: KeyCode,
        pressed: bool,
        modifiers: Modifiers,
    },
    /// Text input was received (IME, composition, etc.)
    TextInput { text: String },
}

/// Mouse button identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

impl MouseButton {
    pub(crate) fn from_raw(button: u8) -> Self {
        match button {
            0 => Self::Left,
            1 => Self::Right,
            2 => Self::Middle,
            other => Self::Other(other),
        }
    }
}

/// Keyboard modifier keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    /// Create a new Modifiers struct
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if any modifier is pressed
    pub fn is_any(&self) -> bool {
        self.shift || self.ctrl || self.alt || self.meta
    }

    pub(crate) fn from_bits(bits: u32) -> Self {
        Self {
            shift: bits & 0b0001 != 0,
            ctrl: bits & 0b0010 != 0,
            alt: bits & 0b0100 != 0,
            meta: bits & 0b1000 != 0,
        }
    }
}

/// Keyboard key codes
///
/// This is a simplified key code enum. For full keyboard support,
/// platform-specific key codes may be needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    // Letters
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

    // Numbers
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

    // Special keys
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

    // Arrow keys
    Up,
    Down,
    Left,
    Right,

    // Function keys
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

    // Modifier keys
    Shift,
    Control,
    Alt,
    Meta,

    // Other
    Unknown(u32),
}

impl KeyCode {
    pub(crate) fn from_raw(code: u32) -> Self {
        match code {
            0 => Self::A,
            1 => Self::B,
            2 => Self::C,
            3 => Self::D,
            4 => Self::E,
            5 => Self::F,
            6 => Self::G,
            7 => Self::H,
            8 => Self::I,
            9 => Self::J,
            10 => Self::K,
            11 => Self::L,
            12 => Self::M,
            13 => Self::N,
            14 => Self::O,
            15 => Self::P,
            16 => Self::Q,
            17 => Self::R,
            18 => Self::S,
            19 => Self::T,
            20 => Self::U,
            21 => Self::V,
            22 => Self::W,
            23 => Self::X,
            24 => Self::Y,
            25 => Self::Z,
            26 => Self::Key0,
            27 => Self::Key1,
            28 => Self::Key2,
            29 => Self::Key3,
            30 => Self::Key4,
            31 => Self::Key5,
            32 => Self::Key6,
            33 => Self::Key7,
            34 => Self::Key8,
            35 => Self::Key9,
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
            51 => Self::F1,
            52 => Self::F2,
            53 => Self::F3,
            54 => Self::F4,
            55 => Self::F5,
            56 => Self::F6,
            57 => Self::F7,
            58 => Self::F8,
            59 => Self::F9,
            60 => Self::F10,
            61 => Self::F11,
            62 => Self::F12,
            63 => Self::Shift,
            64 => Self::Control,
            65 => Self::Alt,
            66 => Self::Meta,
            _ => Self::Unknown(code),
        }
    }
}

/// Event callback type
pub type EventCallback = Arc<dyn Fn(WindowEvent) + Send + Sync>;

/// Event queue for a window
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

    /// Push an event to the queue
    pub fn push(&self, event: WindowEvent) {
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }

    /// Pop all events from the queue
    pub fn pop_all(&self) -> Vec<WindowEvent> {
        let mut events = self.events.lock().unwrap();
        std::mem::take(&mut *events)
    }

    /// Register an event callback
    pub fn register_callback(&self, callback: EventCallback) {
        let mut callbacks = self.callbacks.lock().unwrap();
        callbacks.push(callback);
    }

    /// Process all queued events by calling registered callbacks
    /// Returns the events that were processed (for manual processing if needed)
    ///
    /// Locking order: events -> callbacks (to prevent deadlocks)
    pub fn process_events(&self) -> Vec<WindowEvent> {
        // Step 1: Get all events (releases lock immediately)
        let events = self.pop_all();
        if events.is_empty() {
            return Vec::new();
        }

        // Step 2: Get callbacks (releases lock immediately to avoid holding during callback execution)
        let callbacks: Vec<EventCallback> = {
            let callbacks = self.callbacks.lock().unwrap();
            callbacks.clone()
        };

        // Step 3: Execute callbacks (no locks held, preventing deadlocks)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_queue_push_pop_all() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::CloseRequested);
        queue.push(WindowEvent::Focused);
        let out = queue.pop_all();
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0], WindowEvent::CloseRequested));
        assert!(matches!(out[1], WindowEvent::Focused));
        assert!(queue.pop_all().is_empty());
    }

    #[test]
    fn event_queue_process_events_invokes_callbacks() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::CloseRequested);
        let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let rec = std::sync::Arc::clone(&received);
        queue.register_callback(Arc::new(move |e| {
            rec.lock().unwrap().push(e);
        }));
        let processed = queue.process_events();
        assert_eq!(processed.len(), 1);
        assert_eq!(received.lock().unwrap().len(), 1);
    }

    #[test]
    fn modifiers_from_bits_and_is_any() {
        let none = Modifiers::from_bits(0);
        assert!(!none.is_any());
        assert!(!none.shift && !none.ctrl && !none.alt && !none.meta);

        let shift = Modifiers::from_bits(0b0001);
        assert!(shift.is_any());
        assert!(shift.shift && !shift.ctrl);

        let all = Modifiers::from_bits(0b1111);
        assert!(all.is_any());
        assert!(all.shift && all.ctrl && all.alt && all.meta);
    }

    #[test]
    fn modifiers_default() {
        let m = Modifiers::default();
        assert!(!m.is_any());
    }
}
