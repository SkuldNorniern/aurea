//! Convert a [`NativeWindowHandle`] into ZenGPU's window/display handle pair.

#[cfg(target_os = "linux")]
use super::handles::LinuxWindowHandle;
use super::handles::NativeWindowHandle;
use crate::{AureaError, AureaResult};
#[cfg(target_os = "windows")]
use std::num::NonZeroIsize;
#[cfg(target_os = "linux")]
use std::num::NonZeroU32;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::ptr::NonNull;
#[cfg(target_os = "macos")]
use zen_window_handle::{AppKitWindowHandle, DisplayHandle, WindowHandle};
#[cfg(target_os = "linux")]
use zen_window_handle::{
    DisplayHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle, XcbDisplayHandle,
    XcbWindowHandle,
};
#[cfg(target_os = "windows")]
use zen_window_handle::{DisplayHandle, Win32WindowHandle, WindowHandle};
use zengpu_hal::WindowHandles;

/// Build the `zengpu_hal` window/display handle pair for a native window or
/// canvas handle already extracted via
/// [`native_handle_from_window_ptr`](super::handles::native_handle_from_window_ptr)
/// or [`native_handle_from_canvas_ptr`](super::handles::native_handle_from_canvas_ptr).
pub fn window_handles(native: &NativeWindowHandle) -> AureaResult<WindowHandles> {
    #[cfg(target_os = "macos")]
    {
        let NativeWindowHandle::MacOS { ns_view } = native;
        let view = NonNull::new(*ns_view).ok_or(AureaError::ElementOperationFailed)?;
        return Ok(WindowHandles::from_raw(
            WindowHandle::AppKit(AppKitWindowHandle::new(view)),
            DisplayHandle::AppKit,
        ));
    }
    #[cfg(target_os = "windows")]
    {
        let NativeWindowHandle::Windows { hwnd } = native;
        let hwnd = NonZeroIsize::new(*hwnd as isize).ok_or(AureaError::ElementOperationFailed)?;
        Ok(WindowHandles::from_raw(
            WindowHandle::Win32(Win32WindowHandle::new(hwnd)),
            DisplayHandle::Windows,
        ))
    }
    #[cfg(target_os = "linux")]
    {
        let NativeWindowHandle::Linux(handle) = native;
        return match handle {
            LinuxWindowHandle::Xcb { window, connection } => {
                let window = NonZeroU32::new(*window).ok_or(AureaError::ElementOperationFailed)?;
                let connection =
                    NonNull::new(*connection).ok_or(AureaError::ElementOperationFailed)?;
                Ok(WindowHandles::from_raw(
                    WindowHandle::Xcb(XcbWindowHandle::new(window)),
                    DisplayHandle::Xcb(XcbDisplayHandle {
                        connection: Some(connection),
                    }),
                ))
            }
            LinuxWindowHandle::Wayland { surface, display } => {
                let surface = NonNull::new(*surface).ok_or(AureaError::ElementOperationFailed)?;
                let display = NonNull::new(*display).ok_or(AureaError::ElementOperationFailed)?;
                Ok(WindowHandles::from_raw(
                    WindowHandle::Wayland(WaylandWindowHandle::new(surface)),
                    DisplayHandle::Wayland(WaylandDisplayHandle { display }),
                ))
            }
        };
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // zen-window-handle has no iOS/Android/other representation yet.
        let _ = native;
        Err(AureaError::ElementOperationFailed)
    }
}
