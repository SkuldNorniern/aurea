//! Menu bar and submenu support.

use crate::ffi::*;
use crate::registry::menu::{
    invoke_menu_callback as invoke_registered_menu_callback, next_menu_item_id,
    register_menu_callback,
};
use crate::{AureaError, AureaResult};
use std::{ffi::CString, os::raw::c_void};

use log::debug;

/// A keyboard shortcut key for menu items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    Char(char),
    F(u8),
    Enter,
    Tab,
    Escape,
    Space,
    Backspace,
    Delete,
}

/// A portable menu shortcut description.
///
/// `primary` maps to Command on macOS and Ctrl on Windows/Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuShortcut {
    key: ShortcutKey,
    primary: bool,
    shift: bool,
    alt: bool,
    ctrl: bool,
    meta: bool,
}

impl MenuShortcut {
    pub fn new(key: ShortcutKey) -> Self {
        Self {
            key,
            primary: false,
            shift: false,
            alt: false,
            ctrl: false,
            meta: false,
        }
    }

    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub fn meta(mut self) -> Self {
        self.meta = true;
        self
    }

    fn encode_for_platform(self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if self.primary {
            #[cfg(target_os = "macos")]
            {
                parts.push("Cmd".to_string());
            }
            #[cfg(not(target_os = "macos"))]
            {
                parts.push("Ctrl".to_string());
            }
        }

        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.meta {
            parts.push("Meta".to_string());
        }

        let key = match self.key {
            ShortcutKey::Char(c) => c.to_ascii_uppercase().to_string(),
            ShortcutKey::F(n) => format!("F{}", n),
            ShortcutKey::Enter => "Enter".to_string(),
            ShortcutKey::Tab => "Tab".to_string(),
            ShortcutKey::Escape => "Escape".to_string(),
            ShortcutKey::Space => "Space".to_string(),
            ShortcutKey::Backspace => "Backspace".to_string(),
            ShortcutKey::Delete => "Delete".to_string(),
        };
        parts.push(key);

        parts.join("+")
    }
}

/// A native menu bar attached to a window.
pub struct MenuBar {
    pub handle: *mut c_void,
}

/// A submenu inside a menu bar.
pub struct SubMenu {
    pub handle: *mut c_void,
}

impl MenuBar {
    pub fn new(handle: *mut c_void) -> Self {
        Self { handle }
    }

    /// Add a submenu to this menu bar.
    pub fn add_submenu(&mut self, title: &str) -> AureaResult<SubMenu> {
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_submenu(self.handle, title.as_ptr()) };

        if handle.is_null() {
            return Err(AureaError::MenuItemAddFailed);
        }

        debug!("Added submenu '{}'", title.to_string_lossy());

        Ok(SubMenu { handle })
    }

    /// Return the underlying native handle.
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }
}

impl SubMenu {
    /// Add a clickable menu item with a callback.
    pub fn add_item<F>(&mut self, title: &str, callback: F) -> AureaResult<()>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let id = next_menu_item_id();

        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_add_menu_item(self.handle, title.as_ptr(), id) };

        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }

        register_menu_callback(id, callback);
        debug!(
            "Added menu item '{}' with id {}",
            title.to_string_lossy(),
            id
        );

        Ok(())
    }

    /// Add a clickable menu item with a portable keyboard shortcut.
    pub fn add_item_with_shortcut<F>(
        &mut self,
        title: &str,
        shortcut: MenuShortcut,
        callback: F,
    ) -> AureaResult<()>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let label = format!("{}\t{}", title, shortcut.encode_for_platform());
        self.add_item(&label, callback)
    }

    /// Add a visual separator in the submenu.
    pub fn add_separator(&mut self) -> AureaResult<()> {
        let result = unsafe { ng_platform_add_menu_separator(self.handle) };
        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }
        Ok(())
    }

    /// Return the underlying native handle.
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }
}

pub fn invoke_menu_callback(id: u32) {
    invoke_registered_menu_callback(id);
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
