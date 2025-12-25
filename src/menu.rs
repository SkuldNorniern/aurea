use std::{ffi::CString, os::raw::c_void};
use crate::{AureaError, AureaResult, ffi::*};
use log::debug;

pub struct MenuBar {
    pub(crate) handle: *mut c_void,
    #[allow(dead_code)]
    pub(crate) callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
}

pub struct SubMenu {
    pub(crate) handle: *mut c_void,
    #[allow(dead_code)]
    pub(crate) callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
}

impl MenuBar {
    pub(crate) fn new(handle: *mut c_void) -> Self {
        Self {
            handle,
            callbacks: Vec::new(),
        }
    }
    
    pub fn add_submenu(&mut self, title: &str) -> AureaResult<SubMenu> {
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_submenu(self.handle, title.as_ptr()) };
        
        if handle.is_null() {
            return Err(AureaError::MenuItemAddFailed);
        }
        
        debug!("Added submenu '{}'", title.to_string_lossy());
        
        Ok(SubMenu {
            handle,
            callbacks: Vec::new(),
        })
    }
    
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }
}

impl SubMenu {
    pub fn add_item<F>(&mut self, title: &str, callback: F) -> AureaResult<()>
    where
        F: Fn() + Send + Sync + 'static,
    {
        static mut MENU_ITEM_ID: u32 = 1;
        let id = unsafe {
            MENU_ITEM_ID += 1;
            MENU_ITEM_ID - 1
        };
        
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_add_menu_item(self.handle, title.as_ptr(), id) };
        
        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }
        
        self.callbacks.push(Box::new(callback));
        debug!("Added menu item '{}'", title.to_string_lossy());
        
        Ok(())
    }
    
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                ng_platform_destroy_menu(self.handle);
            }
        }
    }
}

