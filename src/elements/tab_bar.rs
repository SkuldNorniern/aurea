//! Tab bar element with selectable tabs and drag-to-detach support.
//!
//! Provides tab chips (not a dropdown) and, on supported platforms,
//! allows dragging a tab out of the window to create a popup.

use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::{
    ffi::CString,
    os::raw::c_void,
    sync::{LazyLock, Mutex},
};

static TAB_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
static TAB_SELECTED_CALLBACKS: LazyLock<Mutex<std::collections::HashMap<u32, Box<dyn Fn(i32) + Send + Sync>>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
static TAB_DETACH_CALLBACKS: LazyLock<Mutex<std::collections::HashMap<u32, Box<dyn Fn(i32) + Send + Sync>>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

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
        let id = {
            let mut id_guard = crate::sync::lock(&TAB_ID);
            *id_guard += 1;
            *id_guard - 1
        };

        let handle = unsafe { ng_platform_create_tab_bar(id) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        {
            let mut selected = crate::sync::lock(&TAB_SELECTED_CALLBACKS);
            selected.insert(id, Box::new(on_selected));
        }
        {
            let mut detach = crate::sync::lock(&TAB_DETACH_CALLBACKS);
            detach.insert(id, Box::new(on_detach));
        }

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

pub(crate) fn invoke_tab_bar_selected(id: u32, index: i32) {
    let callbacks = crate::sync::lock(&TAB_SELECTED_CALLBACKS);
    if let Some(cb) = callbacks.get(&id) {
        cb(index);
    }
}

pub(crate) fn invoke_tab_bar_detach(id: u32, index: i32) {
    let callbacks = crate::sync::lock(&TAB_DETACH_CALLBACKS);
    if let Some(cb) = callbacks.get(&id) {
        cb(index);
    }
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
