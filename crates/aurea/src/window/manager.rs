//! Window manager for multi-window support
//!
//! This module provides a window registry for managing multiple windows
//! in desktop applications.

use crate::AureaResult;
use crate::window::Window;
use std::sync::{Arc, Mutex};

/// Window manager for tracking multiple windows
///
/// This manager uses a single Mutex to protect the window list, avoiding
/// nested Arc/Mutex patterns that could lead to deadlocks.
pub struct WindowManager {
    windows: Mutex<Vec<Arc<Window>>>,
}

impl WindowManager {
    /// Create a new window manager
    pub fn new() -> Self {
        Self {
            windows: Mutex::new(Vec::new()),
        }
    }

    /// Register a window with the manager
    pub fn register(&self, window: Arc<Window>) {
        let mut windows = crate::sync::lock(&self.windows);
        windows.push(window);
    }

    /// Unregister a window from the manager
    pub fn unregister(&self, window_handle: *mut std::os::raw::c_void) {
        let mut windows = crate::sync::lock(&self.windows);
        windows.retain(|w| w.handle != window_handle);
    }

    /// Get all registered windows
    pub fn windows(&self) -> Vec<Arc<Window>> {
        let windows = crate::sync::lock(&self.windows);
        windows.clone()
    }

    /// Get the number of registered windows
    pub fn count(&self) -> usize {
        let windows = crate::sync::lock(&self.windows);
        windows.len()
    }

    /// Find a window by handle
    pub fn find(&self, handle: *mut std::os::raw::c_void) -> Option<Arc<Window>> {
        let windows = crate::sync::lock(&self.windows);
        windows.iter().find(|w| w.handle == handle).cloned()
    }

    /// Process events for all registered windows
    pub fn poll_all_events(&self) -> Vec<(crate::window::WindowId, crate::window::WindowEvent)> {
        unsafe {
            crate::ffi::ng_platform_poll_events();
        }
        let mut all_events = Vec::new();
        let windows = self.windows();
        for window in windows {
            let events = window.poll_events();
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
