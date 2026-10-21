use aurea_foundation::lock;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

/// Window type for different window behaviors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// Standard application window with title bar, minimize/maximize buttons.
    Normal,
    /// Popup window (borderless or minimal border, stays on top).
    Popup,
    /// Tool window (floating, smaller title bar, stays on top of parent).
    Tool,
    /// Utility window (similar to tool, but different styling).
    Utility,
    /// Sheet window (modal, attached to parent window - macOS).
    Sheet,
    /// Dialog window (modal dialog).
    Dialog,
}

/// A window's identity, for as long as the process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(u64);

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

static IDS_BY_HANDLE: LazyLock<Mutex<HashMap<usize, WindowId>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl WindowId {
    pub(crate) fn claim(handle: *mut c_void) -> Self {
        let id = Self(NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed));
        lock(&IDS_BY_HANDLE).insert(handle as usize, id);
        id
    }

    pub(crate) fn forget(handle: *mut c_void) {
        lock(&IDS_BY_HANDLE).remove(&(handle as usize));
    }

    pub(crate) fn of_handle(handle: *mut c_void) -> Option<Self> {
        lock(&IDS_BY_HANDLE).get(&(handle as usize)).copied()
    }

    pub(crate) fn of_key(key: usize) -> Option<Self> {
        lock(&IDS_BY_HANDLE).get(&key).copied()
    }

    /// The raw value, for logging or as a key of the caller's own.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// The cursor shown over a window's content.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorIcon {
    /// The platform's default cursor.
    #[default]
    Default = 0,
    /// A hand, for something clickable.
    Pointer = 1,
    /// An I-beam, for text that can be edited or selected.
    Text = 2,
    /// A crosshair.
    Crosshair = 3,
    /// Four-way arrows, for moving.
    Move = 4,
    /// Left-right arrows, like a slider or a column edge.
    ResizeHorizontal = 5,
    /// Up-down arrows, like a row edge.
    ResizeVertical = 6,
    /// A circle with a slash, for a blocked action.
    NotAllowed = 7,
    /// Busy.
    Wait = 8,
    /// Diagonal arrows from top-left to bottom-right, like a corner handle.
    ResizeNwSe = 9,
    /// Diagonal arrows from top-right to bottom-left, like a corner handle.
    ResizeNeSw = 10,
}

/// Cursor grab modes.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorGrabMode {
    /// Do not grab the cursor.
    None = 0,
    /// Confine cursor to the window.
    Confined = 1,
    /// Lock cursor to the window and enable raw motion.
    Locked = 2,
}

#[cfg(test)]
mod cursor_tests {
    use super::*;

    #[test]
    fn diagonal_cursors_follow_the_native_table() {
        assert_eq!(CursorIcon::Wait as i32, 8);
        assert_eq!(CursorIcon::ResizeNwSe as i32, 9);
        assert_eq!(CursorIcon::ResizeNeSw as i32, 10);
    }
}
