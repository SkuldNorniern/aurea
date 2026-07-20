//! Platform-native window/display handle extraction.
//!
//! [`NativeWindowHandle`] is Aurea's own handle representation, built from raw
//! platform pointers returned by the FFI layer. It has no dependency on
//! `raw-window-handle` or `wgpu` — those conversions live in [`super::rwh`] and
//! [`super::zengpu`], which consume `NativeWindowHandle` as their common input.

use std::os::raw::c_void;
#[cfg(target_os = "linux")]
use std::ptr::null_mut;

#[cfg(target_os = "macos")]
use crate::ffi::ng_platform_window_get_content_view;
#[cfg(target_os = "linux")]
use crate::ffi::{
    ng_platform_canvas_get_wayland_handle, ng_platform_canvas_get_xcb_handle,
    ng_platform_window_get_wayland_handle, ng_platform_window_get_xcb_handle,
};

/// Platform-specific native window handle.
///
/// This type provides platform-specific window handles for external renderer
/// integration (wgpu, ZenGPU).
///
/// # Safety
///
/// This type is safe to send between threads because window handles are opaque
/// pointers that are only used for surface creation, not for actual window
/// manipulation across threads.
#[derive(Debug, Clone, Copy)]
pub enum NativeWindowHandle {
    #[cfg(target_os = "macos")]
    MacOS { ns_view: *mut c_void },
    #[cfg(target_os = "windows")]
    Windows { hwnd: *mut c_void },
    #[cfg(target_os = "linux")]
    Linux(LinuxWindowHandle),
    #[cfg(target_os = "ios")]
    IOS { ui_view: *mut c_void },
    #[cfg(target_os = "android")]
    Android { native_window: *mut c_void },
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub enum LinuxWindowHandle {
    Xcb {
        window: u32,
        connection: *mut c_void,
    },
    Wayland {
        surface: *mut c_void,
        display: *mut c_void,
    },
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_window_handle_from_ptr(window: *mut c_void) -> Option<LinuxWindowHandle> {
    let mut xcb_window: u32 = 0;
    let mut xcb_connection: *mut c_void = null_mut();
    let has_xcb =
        unsafe { ng_platform_window_get_xcb_handle(window, &mut xcb_window, &mut xcb_connection) }
            != 0;
    if has_xcb && xcb_window != 0 && !xcb_connection.is_null() {
        return Some(LinuxWindowHandle::Xcb {
            window: xcb_window,
            connection: xcb_connection,
        });
    }

    let mut surface: *mut c_void = null_mut();
    let mut display: *mut c_void = null_mut();
    let has_wayland =
        unsafe { ng_platform_window_get_wayland_handle(window, &mut surface, &mut display) } != 0;
    if has_wayland && !surface.is_null() && !display.is_null() {
        return Some(LinuxWindowHandle::Wayland { surface, display });
    }

    None
}

#[cfg(target_os = "linux")]
fn linux_canvas_handle_from_ptr(canvas: *mut c_void) -> Option<LinuxWindowHandle> {
    let mut xcb_window: u32 = 0;
    let mut xcb_connection: *mut c_void = null_mut();
    let has_xcb =
        unsafe { ng_platform_canvas_get_xcb_handle(canvas, &mut xcb_window, &mut xcb_connection) }
            != 0;
    if has_xcb && xcb_window != 0 && !xcb_connection.is_null() {
        return Some(LinuxWindowHandle::Xcb {
            window: xcb_window,
            connection: xcb_connection,
        });
    }

    let mut surface: *mut c_void = null_mut();
    let mut display: *mut c_void = null_mut();
    let has_wayland =
        unsafe { ng_platform_canvas_get_wayland_handle(canvas, &mut surface, &mut display) } != 0;
    if has_wayland && !surface.is_null() && !display.is_null() {
        return Some(LinuxWindowHandle::Wayland { surface, display });
    }

    None
}

pub fn native_handle_from_window_ptr(window: *mut c_void) -> Option<NativeWindowHandle> {
    #[cfg(target_os = "macos")]
    {
        let view_ptr = unsafe { ng_platform_window_get_content_view(window) };
        if view_ptr.is_null() {
            return None;
        }
        return Some(NativeWindowHandle::MacOS { ns_view: view_ptr });
    }
    #[cfg(target_os = "windows")]
    {
        if window.is_null() {
            return None;
        }
        Some(NativeWindowHandle::Windows { hwnd: window })
    }
    #[cfg(target_os = "linux")]
    {
        return linux_window_handle_from_ptr(window).map(NativeWindowHandle::Linux);
    }
    #[cfg(target_os = "ios")]
    {
        if window.is_null() {
            return None;
        }
        return Some(NativeWindowHandle::IOS { ui_view: window });
    }
    #[cfg(target_os = "android")]
    {
        if window.is_null() {
            return None;
        }
        return Some(NativeWindowHandle::Android {
            native_window: window,
        });
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "ios",
        target_os = "android"
    )))]
    {
        let _ = window;
        None
    }
}

pub fn native_handle_from_canvas_ptr(canvas: *mut c_void) -> Option<NativeWindowHandle> {
    #[cfg(target_os = "macos")]
    {
        if canvas.is_null() {
            return None;
        }
        return Some(NativeWindowHandle::MacOS { ns_view: canvas });
    }
    #[cfg(target_os = "windows")]
    {
        if canvas.is_null() {
            return None;
        }
        Some(NativeWindowHandle::Windows { hwnd: canvas })
    }
    #[cfg(target_os = "linux")]
    {
        return linux_canvas_handle_from_ptr(canvas).map(NativeWindowHandle::Linux);
    }
    #[cfg(target_os = "ios")]
    {
        if canvas.is_null() {
            return None;
        }
        return Some(NativeWindowHandle::IOS { ui_view: canvas });
    }
    #[cfg(target_os = "android")]
    {
        if canvas.is_null() {
            return None;
        }
        return Some(NativeWindowHandle::Android {
            native_window: canvas,
        });
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "ios",
        target_os = "android"
    )))]
    {
        let _ = canvas;
        None
    }
}

impl NativeWindowHandle {
    /// Get the raw window handle as a pointer.
    pub fn as_ptr(&self) -> *mut c_void {
        match self {
            #[cfg(target_os = "macos")]
            NativeWindowHandle::MacOS { ns_view } => *ns_view,
            #[cfg(target_os = "windows")]
            NativeWindowHandle::Windows { hwnd } => *hwnd,
            #[cfg(target_os = "linux")]
            NativeWindowHandle::Linux(handle) => match handle {
                LinuxWindowHandle::Xcb { connection, .. } => *connection,
                LinuxWindowHandle::Wayland { surface, .. } => *surface,
            },
            #[cfg(target_os = "ios")]
            NativeWindowHandle::IOS { ui_view } => *ui_view,
            #[cfg(target_os = "android")]
            NativeWindowHandle::Android { native_window } => *native_window,
            #[cfg(not(any(
                target_os = "macos",
                target_os = "windows",
                target_os = "linux",
                target_os = "ios",
                target_os = "android"
            )))]
            _ => std::ptr::null_mut(),
        }
    }
}

// SAFETY: NativeWindowHandle contains raw pointers, but they are only used
// for surface creation on the main thread. The handles themselves don't
// need to be thread-safe for this use case.
unsafe impl Send for NativeWindowHandle {}
unsafe impl Sync for NativeWindowHandle {}
