use std::ffi::CString;
use std::os::raw::{c_char, c_int};

/// Errors that might occur during native GUI operations.
#[derive(Debug)]
pub enum GuiError {
    CreateMenuBarFailed,
    AddMenuItemFailed,
}
impl std::fmt::Display for GuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuiError::CreateMenuBarFailed => write!(f, "Failed to create the menubar"),
            GuiError::AddMenuItemFailed => write!(f, "Failed to add a menu item"),
        }
    }
}

impl std::error::Error for GuiError {}

/// A safe Rust wrapper for the native menubar functionality.
pub struct NativeGui;

extern "C" {
    fn ngui_create_menu_bar() -> c_int;
    fn ngui_add_menu_item(title: *const c_char, callback: extern "C" fn()) -> c_int;
    fn ngui_destroy_menu_bar();
}

impl NativeGui {
    /// Initializes the native menubar.
    ///
    /// # Errors
    ///
    /// Returns `GuiError::CreateMenuBarFailed` if the menubar could not be created.
    pub fn new() -> Result<Self, GuiError> {
        // Call the C function to create the menubar.
        let ret = unsafe { ngui_create_menu_bar() };
        if ret != 0 {
            return Err(GuiError::CreateMenuBarFailed);
        }
        Ok(NativeGui)
    }

    /// Adds a menu item with the given title and callback.
    ///
    /// The callback must be an `extern "C"` function pointer that conforms to the expected signature.
    ///
    /// # Errors
    ///
    /// Returns `GuiError::AddMenuItemFailed` if the menu item could not be added.
    pub fn add_menu_item(&self, title: &str, callback: extern "C" fn()) -> Result<(), GuiError> {
        // Convert the Rust string slice to a C-compatible string.
        // We avoid using unwrap here to properly propagate conversion errors.
        let c_title = CString::new(title).map_err(|_| GuiError::AddMenuItemFailed)?;
        let ret = unsafe { ngui_add_menu_item(c_title.as_ptr(), callback) };
        if ret != 0 {
            return Err(GuiError::AddMenuItemFailed);
        }
        Ok(())
    }
}

impl Drop for NativeGui {
    fn drop(&mut self) {
        // Clean up the native menubar when the Rust object goes out of scope.
        unsafe {
            ngui_destroy_menu_bar();
        }
    }
} 