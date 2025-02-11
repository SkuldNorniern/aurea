use std::ffi::CString;
use std::os::raw::c_void;

use crate::{
    Error, Result,
    window::Window,
    ffi::*
};

 pub struct MenuBar {
    pub handle: *mut c_void,
    pub callbacks: Vec<Box<dyn Fn()>>,
}

impl MenuBar {
    /// Adds a menu item with the given title and callback
    ///
    /// # Errors
    ///
    /// Returns `Error::MenuItemAddFailed` if the menu item could not be added
    /// Returns `Error::InvalidTitle` if the title contains invalid characters
    pub fn add_item<F>(&mut self, title: &str, callback: F) -> Result<()>
    where
        F: Fn() + 'static,
    {
        let title = CString::new(title).map_err(|_| Error::InvalidTitle)?;
        let id = self.callbacks.len() as u32;

        let result = unsafe {
            ng_platform_add_menu_item(self.handle, title.as_ptr(), id)
        };

        if result != 0 {
            return Err(Error::MenuItemAddFailed);
        }

        self.callbacks.push(Box::new(callback));
        Ok(())
    }

    /// Creates a new submenu with the given title
    ///
    /// # Errors
    ///
    /// Returns `Error::MenuCreationFailed` if the submenu could not be created
    /// Returns `Error::InvalidTitle` if the title contains invalid characters
    pub fn add_submenu(&mut self, title: &str) -> Result<SubMenu> {
        let title = CString::new(title).map_err(|_| Error::InvalidTitle)?;
        
        let submenu_handle = unsafe {
            ng_platform_create_submenu(self.handle, title.as_ptr())
        };
        
        if submenu_handle.is_null() {
            return Err(Error::MenuCreationFailed);
        }

        Ok(SubMenu {
            handle: submenu_handle,
            parent: self,
        })
    }
}

/// A submenu in the menu bar
pub struct SubMenu<'a> {
    handle: *mut c_void,
    parent: &'a mut MenuBar,
}

impl<'a> SubMenu<'a> {
    /// Adds a menu item to this submenu
    ///
    /// # Errors
    ///
    /// Returns `Error::MenuItemAddFailed` if the menu item could not be added
    /// Returns `Error::InvalidTitle` if the title contains invalid characters
    pub fn add_item<F>(&mut self, title: &str, callback: F) -> Result<()>
    where
        F: Fn() + 'static,
    {
        let title = CString::new(title).map_err(|_| Error::InvalidTitle)?;
        let id = self.parent.callbacks.len() as u32;

        let result = unsafe {
            ng_platform_add_menu_item(self.handle, title.as_ptr(), id)
        };

        if result != 0 {
            return Err(Error::MenuItemAddFailed);
        }

        self.parent.callbacks.push(Box::new(callback));
        Ok(())
    }
}

impl<'a> Drop for SubMenu<'a> {
    fn drop(&mut self) {
        unsafe {
            ng_platform_destroy_menu(self.handle);
        }
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        unsafe {
            ng_platform_destroy_menu(self.handle);
        }
    }
}


unsafe impl Send for MenuBar {}
unsafe impl Sync for MenuBar {} 