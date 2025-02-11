use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

pub mod elements;

use crate::elements::Element;
/// Errors that might occur during native GUI operations.
#[derive(Debug)]
pub enum Error {
    WindowCreationFailed,
    MenuCreationFailed,
    MenuItemAddFailed,
    InvalidTitle,
    PlatformError(i32),
    EventLoopError,
    ElementOperationFailed,
}

type Result<T> = std::result::Result<T, Error>;

// FFI declarations - minimal platform-specific bindings
extern "C" {
    fn ng_platform_init() -> c_int;
    fn ng_platform_cleanup();
    fn ng_platform_create_window(title: *const c_char, width: c_int, height: c_int) -> *mut c_void;
    fn ng_platform_destroy_window(handle: *mut c_void);
    fn ng_platform_create_menu() -> *mut c_void;
    fn ng_platform_destroy_menu(handle: *mut c_void);
    fn ng_platform_attach_menu(window: *mut c_void, menu: *mut c_void) -> c_int;
    fn ng_platform_add_menu_item(menu: *mut c_void, title: *const c_char, id: u32) -> c_int;
    fn ng_platform_run() -> c_int;
    fn ng_platform_create_submenu(parent: *mut c_void, title: *const c_char) -> *mut c_void;
    fn ng_platform_create_button(title: *const c_char) -> *mut c_void;
    fn ng_platform_create_label(text: *const c_char) -> *mut c_void;
    fn ng_platform_create_box(is_vertical: c_int) -> *mut c_void;
    fn ng_platform_box_add(box_handle: *mut c_void, element: *mut c_void) -> c_int;
    fn ng_platform_set_window_content(window: *mut c_void, content: *mut c_void) -> c_int;
}

/// A native window handle
pub struct Window {
    handle: *mut c_void,
    menu_bar: Option<MenuBar>,
    content: Option<Box<dyn Element>>,
}

/// A native menu bar handle
pub struct MenuBar {
    handle: *mut c_void,
    callbacks: Vec<Box<dyn Fn()>>,
}

impl Window {
    /// Creates a new native window
    ///
    /// # Errors
    ///
    /// Returns `Error::WindowCreationFailed` if the window could not be created
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

    /// Creates and attaches a menu bar to the window
    ///
    /// # Errors
    ///
    /// Returns `Error::MenuCreationFailed` if the menu bar could not be created
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

    /// Run the window's event loop
    ///
    /// # Errors
    ///
    /// Returns `Error::EventLoopError` if the event loop fails
    pub fn run(self) -> Result<()> {
        let result = unsafe { ng_platform_run() };
        if result != 0 {
            return Err(Error::EventLoopError);
        }
        Ok(())
    }

    /// Sets the content of the window
    ///
    /// # Errors
    /// Returns `Error::ElementOperationFailed` if the content cannot be set
    pub fn set_content<E>(&mut self, element: E) -> Result<()> 
    where 
        E: Element + 'static
    {
        let content_handle = element.handle();
        if content_handle.is_null() {
            return Err(Error::ElementOperationFailed);
        }
        
        // Set the content in the native window
        let result = unsafe {
            ng_platform_set_window_content(self.handle, content_handle)
        };
        
        if result != 0 {
            return Err(Error::ElementOperationFailed);
        }
        
        // Store the element to keep it alive
        self.content = Some(Box::new(element));
        Ok(())
    }
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

impl Drop for Window {
    fn drop(&mut self) {
        unsafe { 
            ng_platform_destroy_window(self.handle);
            ng_platform_cleanup();
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

// Implement Send and Sync for Window and MenuBar if the platform supports it
unsafe impl Send for Window {}
unsafe impl Sync for Window {}
unsafe impl Send for MenuBar {}
unsafe impl Sync for MenuBar {} 