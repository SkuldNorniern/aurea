//! Sidebar list element for VS Code / Finder style sidebars.
//!
//! Displays section headers and indented clickable items with selection highlight.

use super::native::NativeElement;
use super::traits::Element;
use crate::registry::elements::{
    next_sidebar_id, register_sidebar_callback, unregister_sidebar_callback,
};
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

pub struct SidebarList {
    handle: NativeElement,
    _id: u32,
}

impl SidebarList {
    pub fn new() -> AureaResult<Self> {
        Self::with_callback(|_| {})
    }

    pub fn with_callback<F>(on_selected: F) -> AureaResult<Self>
    where
        F: Fn(i32) + 'static,
    {
        let id = next_sidebar_id();

        let handle = unsafe { ng_platform_create_sidebar_list(id) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        register_sidebar_callback(id, on_selected);

        Ok(Self {
            handle: NativeElement::new(handle),
            _id: id,
        })
    }

    /// Create a sidebar list and fill it with top-level items.
    pub fn with_items<I, S>(items: I) -> AureaResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut sidebar = Self::new()?;
        sidebar.add_items(items, 0)?;
        Ok(sidebar)
    }

    pub fn add_section(&mut self, title: &str) -> AureaResult<()> {
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let result =
            unsafe { ng_platform_sidebar_list_add_section(self.handle.handle(), title.as_ptr()) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    pub fn add_item(&mut self, title: &str, indent: i32) -> AureaResult<()> {
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe {
            ng_platform_sidebar_list_add_item(self.handle.handle(), title.as_ptr(), indent)
        };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Add multiple items using the same indent level.
    pub fn add_items<I, S>(&mut self, items: I, indent: i32) -> AureaResult<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for item in items {
            self.add_item(item.as_ref(), indent)?;
        }
        Ok(())
    }

    pub fn set_selected(&mut self, index: i32) -> AureaResult<()> {
        let result = unsafe { ng_platform_sidebar_list_set_selected(self.handle.handle(), index) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    pub fn get_selected(&self) -> i32 {
        unsafe { ng_platform_sidebar_list_get_selected(self.handle.handle()) }
    }

    pub fn clear(&mut self) -> AureaResult<()> {
        let result = unsafe { ng_platform_sidebar_list_clear(self.handle.handle()) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }
}

impl Element for SidebarList {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {
        unsafe {
            ng_platform_sidebar_list_invalidate(self.handle.handle());
        }
    }
}

impl Drop for SidebarList {
    fn drop(&mut self) {
        // The registry held the closure for the life of the process otherwise,
        // and it keeps alive whatever the application captured in it.
        unregister_sidebar_callback(self._id);
    }
}
