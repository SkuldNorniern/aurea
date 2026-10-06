//! An icon in the notification area ("system tray").
//!
//! Pairs with [`Window::set_hide_on_close`](crate::Window::set_hide_on_close):
//! closing the window hides it, the icon stays, and clicking the icon or
//! choosing from its menu brings the app back.
//!
//! ```rust,no_run
//! use aurea::{TrayIcon, Window};
//!
//! # fn main() -> aurea::AureaResult<()> {
//! let window = Window::new("App", 800, 600)?;
//! window.set_hide_on_close(true)?;
//! let mut tray = TrayIcon::new("App", || println!("clicked"))?;
//! tray.add_item("Open", || println!("open"))?;
//! tray.add_separator()?;
//! tray.add_item("Quit", || println!("quit"))?;
//! # Ok(())
//! # }
//! ```
//!
//! Only Windows has a tray so far; elsewhere [`TrayIcon::new`] reports
//! [`AureaError::Unsupported`].

use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;

use crate::ffi::*;
use crate::platform::ui_thread;
use crate::registry::menu::{next_menu_item_id, register_menu_callback, unregister_menu_callback};
use crate::{AureaError, AureaResult, Platform};

/// A notification area icon with a tooltip and a menu. Removed when dropped.
pub struct TrayIcon {
    handle: *mut c_void,
    /// the click callback and every menu item's, dropped with the icon.
    ids: Vec<u32>,
}

impl TrayIcon {
    /// Adds an icon showing `tooltip` when hovered; `on_click` runs on a
    /// left click (or Enter on the icon). Until [`set_icon_rgba`]
    /// (Self::set_icon_rgba), it shows the default application icon.
    pub fn new(tooltip: &str, on_click: impl Fn() + 'static) -> AureaResult<Self> {
        ui_thread::check("TrayIcon::new");
        if !cfg!(windows) {
            return Err(AureaError::Unsupported {
                operation: "add a tray icon",
                platform: Platform::current(),
            });
        }
        let tooltip = CString::new(tooltip).map_err(|_| AureaError::InvalidTitle)?;
        let click = next_menu_item_id();
        let handle = unsafe { ng_platform_create_tray(tooltip.as_ptr(), click) };
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        register_menu_callback(click, on_click);
        Ok(Self {
            handle,
            ids: vec![click],
        })
    }

    /// The icon, from tightly packed RGBA8 pixels; 16 to 32 pixels a side
    /// suits most screens, and Windows scales others.
    pub fn set_icon_rgba(&self, rgba: &[u8], width: u32, height: u32) -> AureaResult<()> {
        ui_thread::check("TrayIcon::set_icon_rgba");
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(AureaError::ElementOperationFailed)?;
        if width == 0 || height == 0 || rgba.len() != expected {
            return Err(AureaError::ElementOperationFailed);
        }
        let result =
            unsafe { ng_platform_tray_set_icon_rgba(self.handle, rgba.as_ptr(), width, height) };
        if result != 0 {
            return Err(AureaError::PlatformError(result));
        }
        Ok(())
    }

    /// The text shown when the pointer rests on the icon; Windows keeps
    /// the first 127 characters.
    pub fn set_tooltip(&self, tooltip: &str) -> AureaResult<()> {
        ui_thread::check("TrayIcon::set_tooltip");
        let tooltip = CString::new(tooltip).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_tray_set_tooltip(self.handle, tooltip.as_ptr()) };
        if result != 0 {
            return Err(AureaError::PlatformError(result));
        }
        Ok(())
    }

    /// Adds an item to the menu a right click opens.
    pub fn add_item(&mut self, title: &str, callback: impl Fn() + 'static) -> AureaResult<()> {
        ui_thread::check("TrayIcon::add_item");
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let id = next_menu_item_id();
        let result = unsafe { ng_platform_tray_add_item(self.handle, title.as_ptr(), id) };
        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }
        register_menu_callback(id, callback);
        self.ids.push(id);
        Ok(())
    }

    /// Adds a line between menu items.
    pub fn add_separator(&mut self) -> AureaResult<()> {
        ui_thread::check("TrayIcon::add_separator");
        let result = unsafe { ng_platform_tray_add_item(self.handle, ptr::null(), 0) };
        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }
        Ok(())
    }

    /// Shows a notification from the icon, like "still running here".
    pub fn notify(&self, title: &str, body: &str) -> AureaResult<()> {
        ui_thread::check("TrayIcon::notify");
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let body = CString::new(body).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_tray_notify(self.handle, title.as_ptr(), body.as_ptr()) };
        if result != 0 {
            return Err(AureaError::PlatformError(result));
        }
        Ok(())
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        for id in &self.ids {
            unregister_menu_callback(*id);
        }
        unsafe {
            ng_platform_destroy_tray(self.handle);
        }
    }
}
