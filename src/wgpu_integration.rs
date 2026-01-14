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
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// Platform-specific native window handle
///
/// SAFETY: This type is safe to send between threads because window handles
/// are opaque pointers that are only used for surface creation, not for
/// actual window manipulation across threads.
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
impl HasWindowHandle for NativeWindowHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        match self {
            #[cfg(target_os = "macos")]
            NativeWindowHandle::MacOS { ns_view } => {
                use raw_window_handle::{AppKitWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: ns_view is a valid window handle from Aurea window creation
                let view = NonNull::new(*ns_view as *mut std::ffi::c_void)
                    .expect("Invalid NSView handle");
                unsafe {
                    Ok(WindowHandle::borrow_raw(AppKitWindowHandle::new(view).into()))
                }
            }
            #[cfg(target_os = "windows")]
            NativeWindowHandle::Windows { hwnd } => {
                use raw_window_handle::{WindowHandle, Win32WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: hwnd is a valid window handle from Aurea window creation
                let hwnd_ptr = NonNull::new(*hwnd as *mut std::ffi::c_void)
                    .expect("Invalid HWND handle");
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        Win32WindowHandle::new(hwnd_ptr, None).into(),
                    ))
                }
            }
            #[cfg(target_os = "linux")]
            NativeWindowHandle::Linux { xcb_window } => {
                use raw_window_handle::{WindowHandle, XcbWindowHandle};
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        XcbWindowHandle::new(*xcb_window, std::ptr::null_mut()).into(),
                    ))
                }
            }
            #[cfg(target_os = "ios")]
            NativeWindowHandle::IOS { ui_view } => {
                use raw_window_handle::{UiKitWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: ui_view is a valid window handle from Aurea window creation
                let view = NonNull::new(*ui_view as *mut std::ffi::c_void)
                    .expect("Invalid UIView handle");
                unsafe {
                    Ok(WindowHandle::borrow_raw(UiKitWindowHandle::new(view).into()))
                }
            }
            #[cfg(target_os = "android")]
            NativeWindowHandle::Android { native_window } => {
                use raw_window_handle::{AndroidNdkWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: native_window is a valid window handle from Aurea window creation
                let window = NonNull::new(*native_window as *mut std::ffi::c_void)
                    .expect("Invalid Android window handle");
                unsafe {
                    Ok(WindowHandle::borrow_raw(AndroidNdkWindowHandle::new(window).into()))
                }
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
}

#[cfg(feature = "wgpu")]
impl HasDisplayHandle for NativeWindowHandle {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        match self {
            #[cfg(target_os = "macos")]
            NativeWindowHandle::MacOS { .. } => {
                use raw_window_handle::{AppKitDisplayHandle, DisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(AppKitDisplayHandle::new().into()))
                }
            }
            #[cfg(target_os = "windows")]
            NativeWindowHandle::Windows { .. } => {
                use raw_window_handle::{DisplayHandle, Win32DisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(Win32DisplayHandle::new().into()))
                }
            }
            #[cfg(target_os = "linux")]
            NativeWindowHandle::Linux { .. } => {
                use raw_window_handle::{DisplayHandle, XcbDisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(
                        XcbDisplayHandle::new(std::ptr::null_mut(), 0).into(),
                    ))
                }
            }
            #[cfg(target_os = "ios")]
            NativeWindowHandle::IOS { .. } => {
                use raw_window_handle::{DisplayHandle, UiKitDisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(UiKitDisplayHandle::new().into()))
                }
            }
            #[cfg(target_os = "android")]
            NativeWindowHandle::Android { .. } => {
                use raw_window_handle::{AndroidNdkDisplayHandle, DisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(AndroidNdkDisplayHandle::new().into()))
                }
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
}

// SAFETY: NativeWindowHandle contains raw pointers, but they are only used
// for surface creation on the main thread. The handles themselves don't
// need to be thread-safe for this use case.
unsafe impl Send for NativeWindowHandle {}
unsafe impl Sync for NativeWindowHandle {}


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
impl HasWindowHandle for crate::window::Window {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        // Create handles directly from self.handle to avoid temporary NativeWindowHandle
        match self.platform() {
            #[cfg(target_os = "macos")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::MacOS) => {
                use raw_window_handle::{AppKitWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: self.handle is a valid NSView from window creation
                let view = NonNull::new(self.handle as *mut std::ffi::c_void)
                    .expect("Invalid NSView handle");
                unsafe {
                    Ok(WindowHandle::borrow_raw(AppKitWindowHandle::new(view).into()))
                }
            }
            #[cfg(target_os = "windows")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::Windows) => {
                use raw_window_handle::{WindowHandle, Win32WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: self.handle is a valid HWND from window creation
                let hwnd_ptr = NonNull::new(self.handle as *mut std::ffi::c_void)
                    .expect("Invalid HWND handle");
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        Win32WindowHandle::new(hwnd_ptr, None).into(),
                    ))
                }
            }
            #[cfg(target_os = "linux")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::Linux) => {
                use raw_window_handle::{WindowHandle, XcbWindowHandle};
                // TODO: Get actual X11 window ID from handle
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        XcbWindowHandle::new(0, std::ptr::null_mut()).into(),
                    ))
                }
            }
            #[cfg(target_os = "ios")]
            crate::platform::Platform::Mobile(crate::platform::MobilePlatform::IOS) => {
                use raw_window_handle::{UiKitWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: self.handle is a valid UIView from window creation
                let view = NonNull::new(self.handle as *mut std::ffi::c_void)
                    .expect("Invalid UIView handle");
                unsafe {
                    Ok(WindowHandle::borrow_raw(UiKitWindowHandle::new(view).into()))
                }
            }
            #[cfg(target_os = "android")]
            crate::platform::Platform::Mobile(crate::platform::MobilePlatform::Android) => {
                use raw_window_handle::{AndroidNdkWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: self.handle is a valid Android window from window creation
                let window = NonNull::new(self.handle as *mut std::ffi::c_void)
                    .expect("Invalid Android window handle");
                unsafe {
                    Ok(WindowHandle::borrow_raw(AndroidNdkWindowHandle::new(window).into()))
                }
            }
            _ => Err(raw_window_handle::HandleError::NotSupported),
        }
    }
}

#[cfg(feature = "wgpu")]
impl HasDisplayHandle for crate::window::Window {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        // Create display handles directly based on platform
        match self.platform() {
            #[cfg(target_os = "macos")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::MacOS) => {
                use raw_window_handle::{AppKitDisplayHandle, DisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(AppKitDisplayHandle::new().into()))
                }
            }
            #[cfg(target_os = "windows")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::Windows) => {
                use raw_window_handle::{DisplayHandle, Win32DisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(Win32DisplayHandle::new().into()))
                }
            }
            #[cfg(target_os = "linux")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::Linux) => {
                use raw_window_handle::{DisplayHandle, XcbDisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(
                        XcbDisplayHandle::new(std::ptr::null_mut(), 0).into(),
                    ))
                }
            }
            #[cfg(target_os = "ios")]
            crate::platform::Platform::Mobile(crate::platform::MobilePlatform::IOS) => {
                use raw_window_handle::{DisplayHandle, UiKitDisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(UiKitDisplayHandle::new().into()))
                }
            }
            #[cfg(target_os = "android")]
            crate::platform::Platform::Mobile(crate::platform::MobilePlatform::Android) => {
                use raw_window_handle::{AndroidNdkDisplayHandle, DisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(AndroidNdkDisplayHandle::new().into()))
                }
            }
            _ => Err(raw_window_handle::HandleError::NotSupported),
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
    /// # Alternative: Manual Surface Creation
    ///
    /// If you need more control over surface creation, you can use `native_handle()`:
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    /// use wgpu::Instance;
    /// use raw_window_handle::{HasWindowHandle, HasDisplayHandle};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Window::new("App", 800, 600)?;
    /// let instance = Instance::new(wgpu::InstanceDescriptor::default());
    /// let native_handle = window.native_handle();
    /// let surface_target = wgpu::SurfaceTarget::from(&native_handle);
    /// let surface = instance.create_surface(surface_target)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_wgpu_surface(
        &self,
        instance: &wgpu::Instance,
    ) -> AureaResult<wgpu::Surface<'static>> {
        // Window implements HasWindowHandle and HasDisplayHandle (via native_handle)
        // wgpu's SurfaceTarget::from can create a surface target from such types
        // SAFETY: The window handle is valid for the lifetime of the window.
        // We extend the lifetime to 'static because the window is typically
        // kept alive for the application lifetime, and wgpu surfaces are
        // valid as long as the window exists.
        let surface_target: wgpu::SurfaceTarget<'static> = unsafe {
            std::mem::transmute(wgpu::SurfaceTarget::from(self))
        };

        let surface = instance
            .create_surface(surface_target)
            .map_err(|_| AureaError::ElementOperationFailed)?;

        Ok(surface)
    }
}

