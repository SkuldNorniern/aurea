//! Tab bar element with selectable tabs and drag-to-detach support.
//!
//! Provides tab chips (not a dropdown) and, on supported platforms,
//! allows dragging a tab out of the window to create a popup.

use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

pub struct TabBar {
    handle: *mut c_void,
    _id: u32,
}

/// TabBar is only used from the main UI thread; the native handle is not shared across threads.
unsafe impl Send for TabBar {}
unsafe impl Sync for TabBar {}

impl TabBar {
    pub fn new() -> AureaResult<Self> {
        Self::with_callbacks(|_| {}, |_| {})
    }

    pub fn with_callbacks<F, G>(on_selected: F, on_detach: G) -> AureaResult<Self>
    where
        F: Fn(i32) + Send + Sync + 'static,
        G: Fn(i32) + Send + Sync + 'static,
    {
        let id = crate::registry::elements::next_tab_id();

        let handle = unsafe { ng_platform_create_tab_bar(id) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        crate::registry::elements::register_tab_callbacks(id, on_selected, on_detach);

        Ok(Self { handle, _id: id })
    }

    pub fn add_tab(&mut self, title: &str) -> AureaResult<()> {
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_tab_bar_add_tab(self.handle, title.as_ptr()) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Add multiple tabs in order.
    pub fn add_tabs<I, S>(&mut self, titles: I) -> AureaResult<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for title in titles {
            self.add_tab(title.as_ref())?;
        }
        Ok(())
    }

    pub fn remove_tab(&mut self, index: i32) -> AureaResult<()> {
        let result = unsafe { ng_platform_tab_bar_remove_tab(self.handle, index) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    pub fn set_selected(&mut self, index: i32) -> AureaResult<()> {
        let result = unsafe { ng_platform_tab_bar_set_selected(self.handle, index) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    pub fn get_selected(&self) -> i32 {
        unsafe { ng_platform_tab_bar_get_selected(self.handle) }
    }
}

pub fn invoke_tab_bar_selected(id: u32, index: i32) {
    crate::registry::elements::invoke_tab_selected(id, index);
}

pub fn invoke_tab_bar_detach(id: u32, index: i32) {
    crate::registry::elements::invoke_tab_detach(id, index);
}

impl Element for TabBar {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_tab_bar_invalidate(self.handle);
        }
    }
}
