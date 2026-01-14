//! WebGPU/wgpu integration for external renderers
//!
//! This module provides support for creating wgpu surfaces from Aurea windows,
//! enabling hybrid rendering: Aurea native widgets (CPU rasterizer) + external wgpu content.
//!
//! Note: This is for external wgpu rendering. Aurea's internal Canvas rendering
//! uses CPU rasterizer with event-driven invalidation, not GPU rendering.

use std::os::raw::c_void;

#[cfg(feature = "wgpu")]
use crate::{AureaError, AureaResult};
#[cfg(feature = "wgpu")]
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

/// Platform-specific native window handle
#[derive(Debug, Clone, Copy)]
pub enum NativeWindowHandle {
    #[cfg(target_os = "macos")]
    MacOS {
        ns_view: *mut c_void,
    },
    #[cfg(target_os = "windows")]
    Windows {
        hwnd: *mut c_void,
    },
    #[cfg(target_os = "linux")]
    Linux {
        xcb_window: u32,
    },
    #[cfg(target_os = "ios")]
    IOS {
        ui_view: *mut c_void,
    },
    #[cfg(target_os = "android")]
    Android {
        native_window: *mut c_void,
    },
}

impl NativeWindowHandle {
    /// Get the raw window handle as a pointer
    pub fn as_ptr(&self) -> *mut c_void {
        match self {
            #[cfg(target_os = "macos")]
            NativeWindowHandle::MacOS { ns_view } => *ns_view,
            #[cfg(target_os = "windows")]
            NativeWindowHandle::Windows { hwnd } => *hwnd,
            #[cfg(target_os = "linux")]
            NativeWindowHandle::Linux { xcb_window: _ } => {
                // X11 window ID needs conversion
                std::ptr::null_mut()
            }
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

#[cfg(feature = "wgpu")]
unsafe impl HasRawWindowHandle for NativeWindowHandle {
    fn raw_window_handle(&self) -> Result<RawWindowHandle, raw_window_handle::HandleError> {
        match self {
            #[cfg(target_os = "macos")]
            NativeWindowHandle::MacOS { ns_view } => {
                use raw_window_handle::{AppKitWindowHandle, RawWindowHandle};
                use std::ptr::NonNull;
                // SAFETY: ns_view is a valid window handle from Aurea window creation
                let view = NonNull::new(*ns_view as *mut std::ffi::c_void)
                    .expect("Invalid NSView handle");
                Ok(RawWindowHandle::AppKit(AppKitWindowHandle::new(view)))
            }
            #[cfg(target_os = "windows")]
            NativeWindowHandle::Windows { hwnd } => {
                use raw_window_handle::{RawWindowHandle, Win32WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: hwnd is a valid window handle from Aurea window creation
                let hwnd_ptr = NonNull::new(*hwnd as *mut std::ffi::c_void)
                    .expect("Invalid HWND handle");
                Ok(RawWindowHandle::Win32(Win32WindowHandle::new(
                    hwnd_ptr,
                    None,
                )))
            }
            #[cfg(target_os = "linux")]
            NativeWindowHandle::Linux { xcb_window } => {
                use raw_window_handle::{RawWindowHandle, XcbWindowHandle};
                Ok(RawWindowHandle::Xcb(XcbWindowHandle::new(
                    *xcb_window,
                    std::ptr::null_mut(),
                )))
            }
            #[cfg(target_os = "ios")]
            NativeWindowHandle::IOS { ui_view } => {
                use raw_window_handle::{RawWindowHandle, UiKitWindowHandle};
                use std::ptr::NonNull;
                // SAFETY: ui_view is a valid window handle from Aurea window creation
                let view = NonNull::new(*ui_view as *mut std::ffi::c_void)
                    .expect("Invalid UIView handle");
                Ok(RawWindowHandle::UiKit(UiKitWindowHandle::new(view)))
            }
            #[cfg(target_os = "android")]
            NativeWindowHandle::Android { native_window } => {
                use raw_window_handle::{AndroidNdkWindowHandle, RawWindowHandle};
                use std::ptr::NonNull;
                // SAFETY: native_window is a valid window handle from Aurea window creation
                let window = NonNull::new(*native_window as *mut std::ffi::c_void)
                    .expect("Invalid Android window handle");
                Ok(RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(window)))
            }
            #[cfg(not(any(
                target_os = "macos",
                target_os = "windows",
                target_os = "linux",
                target_os = "ios",
                target_os = "android"
            )))]
            _ => {
                // Unsupported platform - this should never happen due to compile-time checks
                // but we need to handle it for the match expression
                compile_error!("Unsupported platform for wgpu integration")
            }
        }
    }
}

/// Extension trait for Window to provide wgpu integration methods
pub trait Window {
    fn native_handle_impl(&self) -> NativeWindowHandle;
}

impl Window for crate::window::Window {
    fn native_handle_impl(&self) -> NativeWindowHandle {
        #[cfg(target_os = "macos")]
        {
            NativeWindowHandle::MacOS {
                ns_view: self.handle,
            }
        }
        #[cfg(target_os = "windows")]
        {
            NativeWindowHandle::Windows { hwnd: self.handle }
        }
        #[cfg(target_os = "linux")]
        {
            // For Linux, we need to get the X11 window ID from the handle
            // This is a placeholder - actual implementation needs FFI function
            NativeWindowHandle::Linux { xcb_window: 0 }
        }
        #[cfg(target_os = "ios")]
        {
            NativeWindowHandle::IOS {
                ui_view: self.handle,
            }
        }
        #[cfg(target_os = "android")]
        {
            NativeWindowHandle::Android {
                native_window: self.handle,
            }
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "ios",
            target_os = "android"
        )))]
        {
            compile_error!("Unsupported platform for wgpu integration")
        }
    }
}

#[cfg(feature = "wgpu")]
impl crate::window::Window {

    /// Create a wgpu surface from this window
    ///
    /// This creates a wgpu surface for external rendering. The surface can be
    /// used to render wgpu content alongside Aurea native widgets.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    /// use wgpu::Instance;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Window::new("App", 800, 600)?;
    /// let instance = Instance::new(wgpu::InstanceDescriptor::default());
    /// let surface = window.create_wgpu_surface(&instance)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Note
    ///
    /// This method requires proper platform-specific window handle conversion.
    /// The implementation may need platform-specific adjustments for wgpu 0.20.
    /// For now, use `native_handle()` to get the raw handle and create the surface
    /// manually if needed.
    pub fn create_wgpu_surface(
        &self,
        _instance: &wgpu::Instance,
    ) -> AureaResult<wgpu::Surface<'static>> {
        // TODO: Implement proper wgpu 0.20 surface creation
        // This requires platform-specific SurfaceTarget construction
        // For now, users can use native_handle() to get the raw handle
        // and create the surface manually using wgpu's API
        Err(AureaError::ElementOperationFailed)
    }
}

