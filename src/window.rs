
use std::{ffi::CString, os::raw::c_void};

use crate::elements::Element;
use crate::menu::MenuBar;
use crate::{Error, Result};
use crate::ffi::*;

pub struct Window {
    pub handle: *mut c_void,
    pub menu_bar: Option<MenuBar>,
    pub content: Option<Box<dyn Element>>,
}
impl Window {
    /// Creates a new native window.
    pub fn new(title: &str, width: i32, height: i32) -> Result<Self> {
        static INIT: std::sync::Once = std::sync::Once::new();
        let mut error = None;
        
        INIT.call_once(|| {
            if unsafe { ng_platform_init() } != 0 {
                error = Some(Error::PlatformError(1));
            }
        });

        if let Some(err) = error {
            return Err(err);
        }

        let title = CString::new(title).map_err(|_| Error::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_window(title.as_ptr(), width, height) };
        
        if handle.is_null() {
            return Err(Error::WindowCreationFailed);
        }

        Ok(Self {
            handle,
            menu_bar: None,
            content: None,
        })
    }

    /// Creates and attaches a menu bar to the window.
    pub fn create_menu_bar(&mut self) -> Result<MenuBar> {
        let handle = unsafe { ng_platform_create_menu() };
        if handle.is_null() {
            return Err(Error::MenuCreationFailed);
        }

        let result = unsafe { ng_platform_attach_menu(self.handle, handle) };
        if result != 0 {
            unsafe { ng_platform_destroy_menu(handle) };
            return Err(Error::MenuCreationFailed);
        }

        Ok(MenuBar {
            handle,
            callbacks: Vec::new(),
        })
    }

    /// Run the window's event loop.
    pub fn run(self) -> Result<()> {
        let result = unsafe { ng_platform_run() };
        if result != 0 {
            return Err(Error::EventLoopError);
        }
        Ok(())
    }

    /// Sets the content of the window.
    pub fn set_content<E>(&mut self, element: E) -> Result<()> 
    where 
        E: Element + 'static
    {
        let content_handle = element.handle();
        if content_handle.is_null() {
            return Err(Error::ElementOperationFailed);
        }
        
        let result = unsafe {
            ng_platform_set_window_content(self.handle, content_handle)
        };
        
        if result != 0 {
            return Err(Error::ElementOperationFailed);
        }
        
        self.content = Some(Box::new(element));
        Ok(())
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe { 
            ng_platform_destroy_window(self.handle);
            ng_platform_cleanup();
        }
    }
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}