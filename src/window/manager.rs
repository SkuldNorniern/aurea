//! Window manager for multi-window support
//!
//! This module provides a window registry for managing multiple windows
//! in desktop applications.

use crate::AureaResult;
use crate::window::{Window, WindowEvent, WindowId};
use std::cell::RefCell;
use std::os::raw::c_void;
use std::rc::Rc;

/// Window manager for tracking multiple windows
///
/// Windows belong to the UI thread, so the manager does too: a `RefCell` and
/// `Rc` are the right tools here, and a `Mutex` would only buy the appearance
/// of thread-safety over values that cannot cross threads anyway.
pub struct WindowManager {
    windows: RefCell<Vec<Rc<Window>>>,
}

impl WindowManager {
    /// Create a new window manager
    pub fn new() -> Self {
        Self {
            windows: RefCell::new(Vec::new()),
        }
    }

    /// Register a window with the manager
    pub fn register(&self, window: Rc<Window>) {
        self.windows.borrow_mut().push(window);
    }

    /// Unregister a window from the manager
    pub fn unregister(&self, window_handle: *mut c_void) {
        self.windows
            .borrow_mut()
            .retain(|w| w.handle != window_handle);
    }

    /// Get all registered windows
    pub fn windows(&self) -> Vec<Rc<Window>> {
        self.windows.borrow().clone()
    }

    /// Get the number of registered windows
    pub fn count(&self) -> usize {
        self.windows.borrow().len()
    }

    /// Find a window by handle
    pub fn find(&self, handle: *mut c_void) -> Option<Rc<Window>> {
        self.windows
            .borrow()
            .iter()
            .find(|w| w.handle == handle)
            .cloned()
    }

    /// Process events for all registered windows
    pub fn poll_all_events(&self) -> Vec<(WindowId, WindowEvent)> {
        // Pumped once for the process, then each window takes what is its
        // own: the native queue is not per-window, and asking every window to
        // pump it ran the queue once per window.
        super::pump_platform_events();

        let mut all_events = Vec::new();
        // Snapshot before pumping: a handler may register or drop a window,
        // which would otherwise re-enter the borrow.
        let windows = self.windows();
        for window in windows {
            let events = window.drain_events();
            let window_id = window.id();
            all_events.extend(events.into_iter().map(|event| (window_id, event)));
        }
        all_events
    }

    /// Process frames for all registered windows
    pub fn process_all_frames(&self) -> AureaResult<()> {
        let windows = self.windows();
        for window in windows {
            window.process_frames()?;
        }
        Ok(())
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}
