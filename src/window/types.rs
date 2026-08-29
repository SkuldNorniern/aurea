use aurea_foundation::lock;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

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

/// A window's identity, for as long as the process runs.
///
/// Allocated when the window is created and never given out again, so a value
/// that outlives its window refers to nothing rather than to whichever window
/// came afterwards. It used to be the native handle, which the platform is
/// free to hand back out for the next window — the same reuse that
/// [`WindowProxy`](crate::WindowProxy) was given its own ids to avoid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(u64);

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

/// Handles are how the platform names a window; ids are how an application
/// does. This is the only place the two meet.
static IDS_BY_HANDLE: LazyLock<Mutex<HashMap<usize, WindowId>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl WindowId {
    /// Takes the next id, and remembers which handle it belongs to.
    pub(crate) fn claim(handle: *mut c_void) -> Self {
        let id = Self(NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed));
        lock(&IDS_BY_HANDLE).insert(handle as usize, id);
        id
    }

    /// Forgets a handle, so a later window reusing it starts fresh.
    pub(crate) fn forget(handle: *mut c_void) {
        lock(&IDS_BY_HANDLE).remove(&(handle as usize));
    }

    /// The id for a native handle, or `None` if no live window has it.
    pub(crate) fn of_handle(handle: *mut c_void) -> Option<Self> {
        lock(&IDS_BY_HANDLE).get(&(handle as usize)).copied()
    }

    /// The id for a handle-shaped key, or `None` if no live window has it.
    pub(crate) fn of_key(key: usize) -> Option<Self> {
        lock(&IDS_BY_HANDLE).get(&key).copied()
    }

    /// The raw value, for logging or as a key of the caller's own.
    pub fn get(self) -> u64 {
        self.0
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
