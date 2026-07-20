//! `raw-window-handle` trait impls for [`Window`], built on the shared
//! [`super::handles`] extraction and conversion.

use super::handles::{native_handle_from_window_ptr, raw_handles};
use crate::window::Window;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let native = native_handle_from_window_ptr(self.handle).ok_or(HandleError::Unavailable)?;
        let (window, _display) = raw_handles(&native)?;
        // SAFETY: the raw handle was built from a native pointer that outlives
        // this borrow, per NativeWindowHandle's own safety contract.
        unsafe { Ok(WindowHandle::borrow_raw(window)) }
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let native = native_handle_from_window_ptr(self.handle).ok_or(HandleError::Unavailable)?;
        let (_window, display) = raw_handles(&native)?;
        // SAFETY: the raw handle was built from a native pointer that outlives
        // this borrow, per NativeWindowHandle's own safety contract.
        unsafe { Ok(DisplayHandle::borrow_raw(display)) }
    }
}
