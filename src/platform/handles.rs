//! Platform-native window/display handle extraction.

use std::os::raw::c_void;
#[cfg(target_os = "linux")]
use std::ptr::null_mut;

use raw_window_handle::{HandleError, RawDisplayHandle, RawWindowHandle};

#[cfg(target_os = "macos")]
use crate::ffi::ng_platform_window_get_content_view;
#[cfg(all(target_os = "linux", any(feature = "wgpu", feature = "zengpu")))]
use crate::ffi::{ng_platform_canvas_get_wayland_handle, ng_platform_canvas_get_xcb_handle};
#[cfg(target_os = "linux")]
use crate::ffi::{ng_platform_window_get_wayland_handle, ng_platform_window_get_xcb_handle};

/// Platform-specific native window handle.
///
/// # Safety
///
/// This type is safe to send between threads because window handles are opaque
/// pointers that are only used for surface creation, not for actual window
/// manipulation across threads.
#[derive(Debug, Clone, Copy)]
pub enum NativeWindowHandle {
    /// AppKit view.
    #[cfg(target_os = "macos")]
    MacOS {
        /// `NSView` pointer.
        ns_view: *mut c_void,
    },
    /// Win32 window.
    #[cfg(target_os = "windows")]
    Windows {
        /// `HWND`.
        hwnd: *mut c_void,
    },
    /// X11 or Wayland window.
    #[cfg(target_os = "linux")]
    Linux(LinuxWindowHandle),
    /// UIKit view.
    #[cfg(target_os = "ios")]
    IOS {
        /// `UIView` pointer.
        ui_view: *mut c_void,
    },
    /// Android window.
    #[cfg(target_os = "android")]
    Android {
        /// `ANativeWindow` pointer.
        native_window: *mut c_void,
    },
}

/// A Linux window under X11 or Wayland.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub enum LinuxWindowHandle {
    /// X11 window, through XCB.
    Xcb {
        /// XCB window id.
        window: u32,
        /// `xcb_connection_t` pointer.
        connection: *mut c_void,
    },
    /// Wayland surface.
    Wayland {
        /// `wl_surface` pointer.
        surface: *mut c_void,
        /// `wl_display` pointer.
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

#[cfg(all(target_os = "linux", any(feature = "wgpu", feature = "zengpu")))]
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

pub(crate) fn native_handle_from_window_ptr(window: *mut c_void) -> Option<NativeWindowHandle> {
    #[cfg(target_os = "macos")]
    {
        let view_ptr = unsafe { ng_platform_window_get_content_view(window) };
        if view_ptr.is_null() {
            return None;
        }
        Some(NativeWindowHandle::MacOS { ns_view: view_ptr })
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
        linux_window_handle_from_ptr(window).map(NativeWindowHandle::Linux)
    }
    #[cfg(target_os = "ios")]
    {
        if window.is_null() {
            return None;
        }
        return Some(NativeWindowHandle::IOS { ui_view: window });
    }
    #[cfg(target_os = "android")]
    #[allow(clippy::needless_return)]
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

#[cfg(any(feature = "wgpu", all(feature = "zengpu", not(target_os = "macos"))))]
pub(crate) fn native_handle_from_canvas_ptr(canvas: *mut c_void) -> Option<NativeWindowHandle> {
    #[cfg(target_os = "macos")]
    {
        if canvas.is_null() {
            return None;
        }
        Some(NativeWindowHandle::MacOS { ns_view: canvas })
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
        linux_canvas_handle_from_ptr(canvas).map(NativeWindowHandle::Linux)
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

// SAFETY: this is the one place in the crate that still asserts thread-safety
unsafe impl Send for NativeWindowHandle {}
unsafe impl Sync for NativeWindowHandle {}

/// Convert a [`NativeWindowHandle`] to the `raw-window-handle` pair a `wgpu` surface (or any other
/// `raw-window-handle` consumer) needs.
pub fn raw_handles(
    native: &NativeWindowHandle,
) -> Result<(RawWindowHandle, RawDisplayHandle), HandleError> {
    match native {
        #[cfg(target_os = "macos")]
        NativeWindowHandle::MacOS { ns_view } => {
            use raw_window_handle::{AppKitDisplayHandle, AppKitWindowHandle};
            use std::ptr::NonNull;
            // SAFETY: ns_view is a valid window handle from Aurea window creation.
            let view = NonNull::new(*ns_view).ok_or(HandleError::Unavailable)?;
            Ok((
                RawWindowHandle::AppKit(AppKitWindowHandle::new(view)),
                RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
            ))
        }
        #[cfg(target_os = "windows")]
        NativeWindowHandle::Windows { hwnd } => {
            use raw_window_handle::{Win32WindowHandle, WindowsDisplayHandle};
            use std::num::NonZeroIsize;
            // SAFETY: hwnd is a valid HWND from Aurea window creation.
            let hwnd_nz = NonZeroIsize::new(*hwnd as isize).ok_or(HandleError::Unavailable)?;
            Ok((
                RawWindowHandle::Win32(Win32WindowHandle::new(hwnd_nz)),
                RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            ))
        }
        #[cfg(target_os = "linux")]
        NativeWindowHandle::Linux(handle) => match handle {
            LinuxWindowHandle::Xcb { window, connection } => {
                use raw_window_handle::{XcbDisplayHandle, XcbWindowHandle};
                use std::num::NonZeroU32;
                use std::ptr::NonNull;
                let (Some(window), Some(connection)) =
                    (NonZeroU32::new(*window), NonNull::new(*connection))
                else {
                    return Err(HandleError::Unavailable);
                };
                Ok((
                    RawWindowHandle::Xcb(XcbWindowHandle::new(window)),
                    RawDisplayHandle::Xcb(XcbDisplayHandle::new(Some(connection), 0)),
                ))
            }
            LinuxWindowHandle::Wayland { surface, display } => {
                use raw_window_handle::{WaylandDisplayHandle, WaylandWindowHandle};
                use std::ptr::NonNull;
                let (Some(surface), Some(display)) =
                    (NonNull::new(*surface), NonNull::new(*display))
                else {
                    return Err(HandleError::Unavailable);
                };
                Ok((
                    RawWindowHandle::Wayland(WaylandWindowHandle::new(surface)),
                    RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display)),
                ))
            }
        },
        #[cfg(target_os = "ios")]
        NativeWindowHandle::IOS { ui_view } => {
            use raw_window_handle::{UiKitDisplayHandle, UiKitWindowHandle};
            use std::ptr::NonNull;
            // SAFETY: ui_view is a valid window handle from Aurea window creation.
            let view = NonNull::new(*ui_view).ok_or(HandleError::Unavailable)?;
            Ok((
                RawWindowHandle::UiKit(UiKitWindowHandle::new(view)),
                RawDisplayHandle::UiKit(UiKitDisplayHandle::new()),
            ))
        }
        #[cfg(target_os = "android")]
        NativeWindowHandle::Android { native_window } => {
            use raw_window_handle::{AndroidDisplayHandle, AndroidNdkWindowHandle};
            use std::ptr::NonNull;
            // SAFETY: native_window is a valid window handle from Aurea window creation.
            let window = NonNull::new(*native_window).ok_or(HandleError::Unavailable)?;
            Ok((
                RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(window)),
                RawDisplayHandle::Android(AndroidDisplayHandle::new()),
            ))
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "ios",
            target_os = "android"
        )))]
        _ => {
            compile_error!("Unsupported platform for wgpu integration")
        }
    }
}
