//! An icon in the notification area ("system tray"), or on macOS the menu bar's status area.

use crate::ffi::{c_text, c_title};
use std::os::raw::c_void;
use std::ptr;

use crate::ffi::*;
use crate::platform::ui_thread;
use crate::registry::menu::{next_menu_item_id, register_menu_callback, unregister_menu_callback};
use crate::{AureaError, AureaResult, Platform};

/// A notification area icon with a tooltip and a menu.
pub struct TrayIcon {
    handle: *mut c_void,
    ids: Vec<u32>,
}

impl TrayIcon {
    /// Adds an icon showing `tooltip` when hovered; `on_click` runs on a left click (or Enter on
    /// the icon).
    pub fn new(tooltip: &str, on_click: impl Fn() + 'static) -> AureaResult<Self> {
        ui_thread::check("TrayIcon::new");
        if !cfg!(any(windows, target_os = "macos")) {
            return Err(AureaError::Unsupported {
                operation: "add a tray icon",
                platform: Platform::current(),
            });
        }
        let tooltip = c_title(tooltip)?;
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

    /// The icon, from tightly packed RGBA8 pixels; 16 to 32 pixels a side suits most screens, and
    /// both platforms scale others.
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

    /// The text shown when the pointer rests on the icon; Windows keeps the first 127 characters.
    pub fn set_tooltip(&self, tooltip: &str) -> AureaResult<()> {
        ui_thread::check("TrayIcon::set_tooltip");
        let tooltip = c_title(tooltip)?;
        let result = unsafe { ng_platform_tray_set_tooltip(self.handle, tooltip.as_ptr()) };
        if result != 0 {
            return Err(AureaError::PlatformError(result));
        }
        Ok(())
    }

    /// Adds an item to the menu a right click opens.
    pub fn add_item(&mut self, title: &str, callback: impl Fn() + 'static) -> AureaResult<()> {
        ui_thread::check("TrayIcon::add_item");
        let title = c_title(title)?;
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

    /// Shows a notification from the icon, like "still running here". Fails where the user has
    /// turned the app's notifications off.
    pub fn notify(&self, title: &str, body: &str) -> AureaResult<()> {
        ui_thread::check("TrayIcon::notify");
        let title = c_title(title)?;
        let body = c_text(body)?;
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

#[cfg(all(test, windows))]
mod tests {
    use super::TrayIcon;
    use crate::{AureaResult, Window};

    #[test]
    #[ignore = "puts an icon in the notification area; run with --ignored"]
    fn a_tray_icon_takes_items_and_tooltips() -> AureaResult<()> {
        let _window = Window::new("tray", 100, 100)?;
        let mut tray = TrayIcon::new("Aurea test", || {})?;
        tray.add_item("Save & Load", || {})?;
        tray.add_separator()?;
        tray.set_tooltip("Aurea test 2")?;
        tray.set_icon_rgba(&[255; 16 * 16 * 4], 16, 16)?;
        Ok(())
    }
}
