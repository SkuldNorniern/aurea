//! Convert a [`NativeWindowHandle`] into ZenGPU's window/display handle pair.

#[cfg(target_os = "linux")]
use super::handles::LinuxWindowHandle;
use super::handles::NativeWindowHandle;
use crate::{AureaError, AureaResult};

/// Build the `zengpu_hal` window/display handle pair for a native window or
/// canvas handle already extracted via
/// [`native_handle_from_window_ptr`](super::handles::native_handle_from_window_ptr)
/// or [`native_handle_from_canvas_ptr`](super::handles::native_handle_from_canvas_ptr).
pub fn window_handles(native: &NativeWindowHandle) -> AureaResult<zengpu_hal::WindowHandles> {
    #[cfg(target_os = "macos")]
    {
        let NativeWindowHandle::MacOS { ns_view } = native;
        let view = std::ptr::NonNull::new(*ns_view).ok_or(AureaError::ElementOperationFailed)?;
        return Ok(zengpu_hal::WindowHandles::from_raw(
            zen_window_handle::WindowHandle::AppKit(zen_window_handle::AppKitWindowHandle::new(
                view,
            )),
            zen_window_handle::DisplayHandle::AppKit,
        ));
    }
    #[cfg(target_os = "windows")]
    {
        let NativeWindowHandle::Windows { hwnd } = native;
        let hwnd = std::num::NonZeroIsize::new(*hwnd as isize)
            .ok_or(AureaError::ElementOperationFailed)?;
        return Ok(zengpu_hal::WindowHandles::from_raw(
            zen_window_handle::WindowHandle::Win32(zen_window_handle::Win32WindowHandle::new(
                hwnd,
            )),
            zen_window_handle::DisplayHandle::Windows,
        ));
    }
    #[cfg(target_os = "linux")]
    {
        let NativeWindowHandle::Linux(handle) = native;
        return match handle {
            LinuxWindowHandle::Xcb { window, connection } => {
                let window = std::num::NonZeroU32::new(*window)
                    .ok_or(AureaError::ElementOperationFailed)?;
                let connection = std::ptr::NonNull::new(*connection)
                    .ok_or(AureaError::ElementOperationFailed)?;
                Ok(zengpu_hal::WindowHandles::from_raw(
                    zen_window_handle::WindowHandle::Xcb(zen_window_handle::XcbWindowHandle::new(
                        window,
                    )),
                    zen_window_handle::DisplayHandle::Xcb(zen_window_handle::XcbDisplayHandle {
                        connection: Some(connection),
                    }),
                ))
            }
            LinuxWindowHandle::Wayland { surface, display } => {
                let surface = std::ptr::NonNull::new(*surface)
                    .ok_or(AureaError::ElementOperationFailed)?;
                let display = std::ptr::NonNull::new(*display)
                    .ok_or(AureaError::ElementOperationFailed)?;
                Ok(zengpu_hal::WindowHandles::from_raw(
                    zen_window_handle::WindowHandle::Wayland(
                        zen_window_handle::WaylandWindowHandle::new(surface),
                    ),
                    zen_window_handle::DisplayHandle::Wayland(
                        zen_window_handle::WaylandDisplayHandle { display },
                    ),
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
