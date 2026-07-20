use std::os::raw::c_void;

/// Window type for different window behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// Standard application window with title bar, minimize/maximize buttons
    Normal,
    /// Popup window (borderless or minimal border, stays on top)
    Popup,
    /// Tool window (floating, smaller title bar, stays on top of parent)
    Tool,
    /// Utility window (similar to tool, but different styling)
    Utility,
    /// Sheet window (modal, attached to parent window - macOS)
    Sheet,
    /// Dialog window (modal dialog)
    Dialog,
}

/// Stable window identifier derived from the native handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(usize);

impl WindowId {
    pub fn from_handle(handle: *mut c_void) -> Self {
        Self(handle as usize)
    }

    pub fn from_raw(raw: usize) -> Self {
        Self(raw)
    }
}

/// Cursor grab modes
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorGrabMode {
    /// Do not grab the cursor
    None = 0,
    /// Confine cursor to the window
    Confined = 1,
    /// Lock cursor to the window and enable raw motion
    Locked = 2,
}
