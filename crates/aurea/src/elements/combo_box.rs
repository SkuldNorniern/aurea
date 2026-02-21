use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

pub struct ComboBox {
    handle: *mut c_void,
}

impl ComboBox {
    pub fn new() -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_combo_box() };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self { handle })
    }

    /// Create a combo box and populate it with items.
    pub fn with_items<I, S>(items: I) -> AureaResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut combo = Self::new()?;
        combo.add_items(items)?;
        Ok(combo)
    }

    /// Create a combo box, populate items, and select an index.
    pub fn with_items_selected<I, S>(items: I, selected: i32) -> AureaResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut combo = Self::with_items(items)?;
        combo.set_selected(selected)?;
        Ok(combo)
    }

    pub fn add_item(&mut self, item: &str) -> AureaResult<()> {
        let item = CString::new(item).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_combo_box_add_item(self.handle, item.as_ptr()) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// Add multiple items in insertion order.
    pub fn add_items<I, S>(&mut self, items: I) -> AureaResult<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for item in items {
            self.add_item(item.as_ref())?;
        }
        Ok(())
    }

    pub fn set_selected(&mut self, index: i32) -> AureaResult<()> {
        let result = unsafe { ng_platform_combo_box_set_selected(self.handle, index) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    pub fn get_selected(&self) -> i32 {
        unsafe { ng_platform_combo_box_get_selected(self.handle) }
    }

    pub fn clear(&mut self) -> AureaResult<()> {
        let result = unsafe { ng_platform_combo_box_clear(self.handle) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    pub fn set_enabled(&mut self, enabled: bool) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_combo_box_set_enabled(self.handle, if enabled { 1 } else { 0 }) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }
}

impl Element for ComboBox {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_combo_box_invalidate(self.handle);
        }
    }
}
