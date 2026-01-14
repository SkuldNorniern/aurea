use crate::ffi::*;
use crate::{AureaError, AureaResult};
use std::{
    collections::HashMap,
    ffi::CString,
    os::raw::c_void,
    sync::{LazyLock, Mutex},
};

use log::debug;

static MENU_CALLBACKS: LazyLock<Mutex<HashMap<u32, Box<dyn Fn() + Send + Sync>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct MenuBar {
    pub(crate) handle: *mut c_void,
}

pub struct SubMenu {
    pub(crate) handle: *mut c_void,
}

impl MenuBar {
    pub(crate) fn new(handle: *mut c_void) -> Self {
        Self { handle }
    }

    pub fn add_submenu(&mut self, title: &str) -> AureaResult<SubMenu> {
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_submenu(self.handle, title.as_ptr()) };

        if handle.is_null() {
            return Err(AureaError::MenuItemAddFailed);
        }

        debug!("Added submenu '{}'", title.to_string_lossy());

        Ok(SubMenu { handle })
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
        static MENU_ITEM_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
        let id = {
            let mut id_guard = MENU_ITEM_ID.lock().unwrap();
            *id_guard += 1;
            *id_guard - 1
        };

        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_add_menu_item(self.handle, title.as_ptr(), id) };

        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }

        let mut callbacks = MENU_CALLBACKS.lock().unwrap();
        callbacks.insert(id, Box::new(callback));
        debug!(
            "Added menu item '{}' with id {}",
            title.to_string_lossy(),
            id
        );

        Ok(())
    }

    pub fn handle(&self) -> *mut c_void {
        self.handle
    }
}

pub(crate) fn invoke_menu_callback(id: u32) {
    let callbacks = MENU_CALLBACKS.lock().unwrap();
    if let Some(callback) = callbacks.get(&id) {
        callback();
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
