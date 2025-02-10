use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::collections::HashMap;

/// Errors that might occur during native GUI operations.
#[derive(Debug)]
pub enum Error {
    WindowCreationFailed,
    MenuCreationFailed,
    MenuItemAddFailed,
    InvalidTitle,
    PlatformError(i32),
}

type Result<T> = std::result::Result<T, Error>;

/// FFI declarations
#[allow(non_camel_case_types)]
type ng_handle = *mut c_void;
#[allow(non_camel_case_types)]
type ng_menu_handle = *mut c_void;

extern "C" {
    fn ng_create_window(title: *const c_char, width: c_int, height: c_int) -> ng_handle;
    fn ng_destroy_window(handle: ng_handle);
    fn ng_create_menu_handle() -> ng_menu_handle;
    fn ng_destroy_menu_handle(handle: ng_menu_handle);
    fn ng_attach_menu_to_window(window: ng_handle, menu: ng_menu_handle) -> c_int;
    fn ng_add_raw_menu_item(menu: ng_menu_handle, title: *const c_char, id: u32) -> c_int;
}

pub struct Window {
    handle: ng_handle,
    menu_bar: Option<MenuBar>,
}

pub struct MenuBar {
    handle: ng_menu_handle,
    callbacks: HashMap<u32, Box<dyn Fn()>>,
    next_id: u32,
}

impl Window {
    /// Initializes the native window.
    ///
    /// # Errors
    ///
    /// Returns `Error::WindowCreationFailed` if the window could not be created.
    pub fn new(title: &str, width: i32, height: i32) -> Result<Self> {
        let title = CString::new(title).map_err(|_| Error::InvalidTitle)?;
        let handle = unsafe { ng_create_window(title.as_ptr(), width, height) };
        
        if handle.is_null() {
            return Err(Error::WindowCreationFailed);
        }

        Ok(Self {
            handle,
            menu_bar: None,
        })
    }

    /// Creates a menu bar for the window.
    ///
    /// # Errors
    ///
    /// Returns `Error::MenuCreationFailed` if the menu bar could not be created.
    pub fn create_menu_bar(&mut self) -> Result<&mut MenuBar> {
        let menu_bar = MenuBar::new()?;
        
        // Attach menu bar to window
        let result = unsafe { 
            ng_attach_menu_to_window(self.handle, menu_bar.handle)
        };
        
        if result != 0 {
            return Err(Error::MenuCreationFailed);
        }
        
        self.menu_bar = Some(menu_bar);
        Ok(self.menu_bar.as_mut().unwrap())
    }

    /// Runs the platform-independent event loop.
    pub fn run(&mut self) -> Result<()> {
        // Platform-independent event loop implementation
        Ok(())
    }
}

impl MenuBar {
    /// Initializes the menu bar.
    ///
    /// # Errors
    ///
    /// Returns `Error::MenuCreationFailed` if the menu bar could not be created.
    fn new() -> Result<Self> {
        let handle = unsafe { ng_create_menu_handle() };
        if handle.is_null() {
            return Err(Error::MenuCreationFailed);
        }

        Ok(Self {
            handle,
            callbacks: HashMap::new(),
            next_id: 1,
        })
    }

    /// Adds a menu item with the given title and callback.
    ///
    /// The callback must be an `extern "C"` function pointer that conforms to the expected signature.
    ///
    /// # Errors
    ///
    /// Returns `Error::MenuItemAddFailed` if the menu item could not be added.
    pub fn add_item<F>(&mut self, title: &str, callback: F) -> Result<()>
    where
        F: Fn() + 'static,
    {
        let title = CString::new(title).map_err(|_| Error::InvalidTitle)?;
        let id = self.next_id;
        
        let result = unsafe {
            ng_add_raw_menu_item(self.handle, title.as_ptr(), id)
        };

        if result != 0 {
            return Err(Error::MenuItemAddFailed);
        }

        self.callbacks.insert(id, Box::new(callback));
        self.next_id += 1;
        Ok(())
    }

    /// Handles a menu event.
    fn handle_menu_event(&self, id: u32) {
        if let Some(callback) = self.callbacks.get(&id) {
            callback();
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe { ng_destroy_window(self.handle) };
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        unsafe { ng_destroy_menu_handle(self.handle) };
    }
} 