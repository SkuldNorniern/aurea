//! WebGPU/wgpu integration for external renderers
//!
//! This module provides support for creating wgpu surfaces from Aurea windows,
//! enabling hybrid rendering: Aurea native widgets (CPU rasterizer) + external wgpu content.
//!
//! # Example
//!
//! ```rust,no_run
//! use aurea::Window;
//! use wgpu::Instance;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let window = Window::new("App", 800, 600)?;
//! let instance = Instance::new(wgpu::InstanceDescriptor::default());
//! let surface = window.create_wgpu_surface(&instance)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Manual Surface Creation
//!
//! If you need more control over surface creation, you can use `native_handle()`:
//!
//! ```rust,no_run
//! use aurea::Window;
//! use wgpu::Instance;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let window = Window::new("App", 800, 600)?;
//! let instance = Instance::new(wgpu::InstanceDescriptor::default());
//! let native_handle = window.native_handle();
//! let surface_target = wgpu::SurfaceTarget::from(&native_handle);
//! let surface = instance.create_surface(surface_target)?;
//! # Ok(())
//! # }
//! ```
//!
//! Note: This is for external wgpu rendering. Aurea's internal Canvas rendering
//! uses CPU rasterizer with event-driven invalidation, not GPU rendering.
//!
//! # Surface loss and recreation
//!
//! Two paths emit `WindowEvent::SurfaceLost` and `WindowEvent::SurfaceRecreated`:
//!
//! 1. **Platform lifecycle** – The OS (macOS, Windows, Linux, Android) may invalidate
//!    the surface (e.g. display sleep, mode change). Aurea's lifecycle bridge pushes
//!    `SurfaceLost` / `SurfaceRecreated` into the window event queue. Handle them in
//!    `Window::on_event()`: on `SurfaceLost`, drop or reconfigure the wgpu surface;
//!    on `SurfaceRecreated`, create a new surface and call
//!    `aurea::integration::wgpu::notify_surface_recreated_for_window()` (or
//!    `_for_canvas`) so redraw is scheduled.
//!
//! 2. **wgpu API errors** – When `Surface::get_current_texture()` returns an error,
//!    call `handle_surface_error_for_window()` (or `handle_surface_error_for_canvas()`).
//!    It pushes `SurfaceLost` and returns `SurfaceErrorAction` (Recreate / Skip / Fatal).
//!    If you recreate the surface, then call `notify_surface_recreated_for_window()`
//!    (or `_for_canvas()`).

use std::os::raw::c_void;

#[cfg(feature = "wgpu")]
use crate::{AureaError, AureaResult};
#[cfg(feature = "wgpu")]
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// Platform-specific native window handle
///
/// This type provides platform-specific window handles for external renderer integration.
/// It implements `HasWindowHandle` and `HasDisplayHandle` for use with wgpu and other
/// rendering APIs that require raw window handles.
///
/// # Safety
///
/// This type is safe to send between threads because window handles are opaque pointers
/// that are only used for surface creation, not for actual window manipulation across threads.
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

#[cfg(all(feature = "wgpu", target_os = "linux"))]
fn linux_window_handle_from_ptr(window: *mut c_void) -> Option<LinuxWindowHandle> {
    let mut xcb_window: u32 = 0;
    let mut xcb_connection: *mut c_void = std::ptr::null_mut();
    let has_xcb = unsafe {
        crate::ffi::ng_platform_window_get_xcb_handle(window, &mut xcb_window, &mut xcb_connection)
    } != 0;
    if has_xcb && xcb_window != 0 && !xcb_connection.is_null() {
        return Some(LinuxWindowHandle::Xcb {
            window: xcb_window,
            connection: xcb_connection,
        });
    }

    let mut surface: *mut c_void = std::ptr::null_mut();
    let mut display: *mut c_void = std::ptr::null_mut();
    let has_wayland = unsafe {
        crate::ffi::ng_platform_window_get_wayland_handle(window, &mut surface, &mut display)
    } != 0;
    if has_wayland && !surface.is_null() && !display.is_null() {
        return Some(LinuxWindowHandle::Wayland { surface, display });
    }

    None
}

#[cfg(all(feature = "wgpu", target_os = "linux"))]
fn linux_canvas_handle_from_ptr(canvas: *mut c_void) -> Option<LinuxWindowHandle> {
    let mut xcb_window: u32 = 0;
    let mut xcb_connection: *mut c_void = std::ptr::null_mut();
    let has_xcb = unsafe {
        crate::ffi::ng_platform_canvas_get_xcb_handle(canvas, &mut xcb_window, &mut xcb_connection)
    } != 0;
    if has_xcb && xcb_window != 0 && !xcb_connection.is_null() {
        return Some(LinuxWindowHandle::Xcb {
            window: xcb_window,
            connection: xcb_connection,
        });
    }

    let mut surface: *mut c_void = std::ptr::null_mut();
    let mut display: *mut c_void = std::ptr::null_mut();
    let has_wayland = unsafe {
        crate::ffi::ng_platform_canvas_get_wayland_handle(canvas, &mut surface, &mut display)
    } != 0;
    if has_wayland && !surface.is_null() && !display.is_null() {
        return Some(LinuxWindowHandle::Wayland { surface, display });
    }

    None
}

#[cfg(feature = "wgpu")]
pub fn native_handle_from_window_ptr(window: *mut c_void) -> Option<NativeWindowHandle> {
    #[cfg(target_os = "macos")]
    {
        let view_ptr = unsafe { crate::ffi::ng_platform_window_get_content_view(window) };
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
        return Some(NativeWindowHandle::Windows { hwnd: window });
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

#[cfg(feature = "wgpu")]
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
        return Some(NativeWindowHandle::Windows { hwnd: canvas });
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
    /// Get the raw window handle as a pointer
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
                    .ok_or(raw_window_handle::HandleError::Unavailable)?;
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        AppKitWindowHandle::new(view).into(),
                    ))
                }
            }
            #[cfg(target_os = "windows")]
            NativeWindowHandle::Windows { hwnd } => {
                use raw_window_handle::{Win32WindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: hwnd is a valid window handle from Aurea window creation
                let hwnd_ptr = NonNull::new(*hwnd as *mut std::ffi::c_void)
                    .ok_or(raw_window_handle::HandleError::Unavailable)?;
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        Win32WindowHandle::new(hwnd_ptr, None).into(),
                    ))
                }
            }
            #[cfg(target_os = "linux")]
            NativeWindowHandle::Linux(handle) => match handle {
                LinuxWindowHandle::Xcb { window, connection } => {
                    use raw_window_handle::{WindowHandle, XcbWindowHandle};
                    if *window == 0 || connection.is_null() {
                        return Err(raw_window_handle::HandleError::Unavailable);
                    }
                    unsafe {
                        Ok(WindowHandle::borrow_raw(
                            XcbWindowHandle::new(*window, *connection).into(),
                        ))
                    }
                }
                LinuxWindowHandle::Wayland { surface, .. } => {
                    use raw_window_handle::{WaylandWindowHandle, WindowHandle};
                    if surface.is_null() {
                        return Err(raw_window_handle::HandleError::Unavailable);
                    }
                    unsafe {
                        Ok(WindowHandle::borrow_raw(
                            WaylandWindowHandle::new(*surface).into(),
                        ))
                    }
                }
            },
            #[cfg(target_os = "ios")]
            NativeWindowHandle::IOS { ui_view } => {
                use raw_window_handle::{UiKitWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: ui_view is a valid window handle from Aurea window creation
                let view = NonNull::new(*ui_view as *mut std::ffi::c_void)
                    .ok_or(raw_window_handle::HandleError::Unavailable)?;
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        UiKitWindowHandle::new(view).into(),
                    ))
                }
            }
            #[cfg(target_os = "android")]
            NativeWindowHandle::Android { native_window } => {
                use raw_window_handle::{AndroidNdkWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: native_window is a valid window handle from Aurea window creation
                let window = NonNull::new(*native_window as *mut std::ffi::c_void)
                    .ok_or(raw_window_handle::HandleError::Unavailable)?;
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        AndroidNdkWindowHandle::new(window).into(),
                    ))
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
                unsafe { Ok(DisplayHandle::borrow_raw(AppKitDisplayHandle::new().into())) }
            }
            #[cfg(target_os = "windows")]
            NativeWindowHandle::Windows { .. } => {
                use raw_window_handle::{DisplayHandle, Win32DisplayHandle};
                unsafe { Ok(DisplayHandle::borrow_raw(Win32DisplayHandle::new().into())) }
            }
            #[cfg(target_os = "linux")]
            NativeWindowHandle::Linux(handle) => match handle {
                LinuxWindowHandle::Xcb { connection, .. } => {
                    use raw_window_handle::{DisplayHandle, XcbDisplayHandle};
                    if connection.is_null() {
                        return Err(raw_window_handle::HandleError::Unavailable);
                    }
                    unsafe {
                        Ok(DisplayHandle::borrow_raw(
                            XcbDisplayHandle::new(*connection, 0).into(),
                        ))
                    }
                }
                LinuxWindowHandle::Wayland { display, .. } => {
                    use raw_window_handle::{DisplayHandle, WaylandDisplayHandle};
                    if display.is_null() {
                        return Err(raw_window_handle::HandleError::Unavailable);
                    }
                    unsafe {
                        Ok(DisplayHandle::borrow_raw(
                            WaylandDisplayHandle::new(*display).into(),
                        ))
                    }
                }
            },
            #[cfg(target_os = "ios")]
            NativeWindowHandle::IOS { .. } => {
                use raw_window_handle::{DisplayHandle, UiKitDisplayHandle};
                unsafe { Ok(DisplayHandle::borrow_raw(UiKitDisplayHandle::new().into())) }
            }
            #[cfg(target_os = "android")]
            NativeWindowHandle::Android { .. } => {
                use raw_window_handle::{AndroidNdkDisplayHandle, DisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(
                        AndroidNdkDisplayHandle::new().into(),
                    ))
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

/// Trait for Window to provide native handle implementation
///
/// This trait is used internally to implement `Window::native_handle()`.
#[cfg(feature = "wgpu")]
pub(crate) trait WindowNativeHandle {
    fn native_handle_impl(&self) -> NativeWindowHandle;
}

#[cfg(feature = "wgpu")]
impl WindowNativeHandle for crate::window::Window {
    fn native_handle_impl(&self) -> NativeWindowHandle {
        #[cfg(target_os = "macos")]
        {
            let view_ptr = unsafe { crate::ffi::ng_platform_window_get_content_view(self.handle) };
            NativeWindowHandle::MacOS { ns_view: view_ptr }
        }
        #[cfg(target_os = "windows")]
        {
            NativeWindowHandle::Windows { hwnd: self.handle }
        }
        #[cfg(target_os = "linux")]
        {
            linux_window_handle_from_ptr(self.handle)
                .map(NativeWindowHandle::Linux)
                .unwrap_or(NativeWindowHandle::Linux(LinuxWindowHandle::Xcb {
                    window: 0,
                    connection: std::ptr::null_mut(),
                }))
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceErrorAction {
    /// Surface should be reconfigured/recreated.
    Recreate,
    /// Transient error; skip this frame.
    Skip,
    /// Fatal error; surface cannot be recovered.
    Fatal,
}

#[cfg(feature = "wgpu")]
pub fn handle_surface_error_for_handle(
    handle: *mut c_void,
    error: wgpu::SurfaceError,
) -> SurfaceErrorAction {
    use crate::window::WindowEvent;

    match error {
        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
            if !handle.is_null() {
                crate::window::push_window_event(handle, WindowEvent::SurfaceLost);
                crate::view::FrameScheduler::schedule();
            }
            SurfaceErrorAction::Recreate
        }
        wgpu::SurfaceError::Timeout => SurfaceErrorAction::Skip,
        wgpu::SurfaceError::OutOfMemory => SurfaceErrorAction::Fatal,
        wgpu::SurfaceError::Other => SurfaceErrorAction::Fatal,
    }
}

/// Call this when `Surface::get_current_texture()` fails for a window-backed surface.
/// Pushes `WindowEvent::SurfaceLost` and returns Recreate/Skip/Fatal.
#[cfg(feature = "wgpu")]
pub fn handle_surface_error_for_window(
    window: &crate::window::Window,
    error: wgpu::SurfaceError,
) -> SurfaceErrorAction {
    handle_surface_error_for_handle(window.handle(), error)
}

/// Call this when `Surface::get_current_texture()` fails for a canvas-backed surface.
/// Pushes `WindowEvent::SurfaceLost` and returns Recreate/Skip/Fatal.
#[cfg(feature = "wgpu")]
pub fn handle_surface_error_for_canvas(
    canvas: &crate::render::Canvas,
    error: wgpu::SurfaceError,
) -> SurfaceErrorAction {
    handle_surface_error_for_handle(canvas.window_handle(), error)
}

/// Call after recreating a window-backed wgpu surface so `SurfaceRecreated` is emitted and redraw scheduled.
#[cfg(feature = "wgpu")]
pub fn notify_surface_recreated_for_window(window: &crate::window::Window) {
    use crate::window::WindowEvent;
    crate::window::push_window_event(window.handle(), WindowEvent::SurfaceRecreated);
    crate::view::FrameScheduler::schedule();
}

#[cfg(feature = "wgpu")]
pub fn notify_surface_recreated_for_handle(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    use crate::window::WindowEvent;
    crate::window::push_window_event(handle, WindowEvent::SurfaceRecreated);
    crate::view::FrameScheduler::schedule();
}

/// Call after recreating a canvas-backed wgpu surface so `SurfaceRecreated` is emitted and redraw scheduled.
#[cfg(feature = "wgpu")]
pub fn notify_surface_recreated_for_canvas(canvas: &crate::render::Canvas) {
    let handle = canvas.window_handle();
    if handle.is_null() {
        return;
    }
    use crate::window::WindowEvent;
    crate::window::push_window_event(handle, WindowEvent::SurfaceRecreated);
    crate::view::FrameScheduler::schedule();
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
                // SAFETY: ng_platform_window_get_content_view returns the NSView handle
                let view_ptr =
                    unsafe { crate::ffi::ng_platform_window_get_content_view(self.handle) };
                let view =
                    NonNull::new(view_ptr).ok_or(raw_window_handle::HandleError::Unavailable)?;
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        AppKitWindowHandle::new(view).into(),
                    ))
                }
            }
            #[cfg(target_os = "windows")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::Windows) => {
                use raw_window_handle::{Win32WindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: self.handle is a valid HWND from window creation
                let hwnd_ptr = NonNull::new(self.handle as *mut std::ffi::c_void)
                    .ok_or(raw_window_handle::HandleError::Unavailable)?;
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        Win32WindowHandle::new(hwnd_ptr, None).into(),
                    ))
                }
            }
            #[cfg(target_os = "linux")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::Linux) => {
                let handle = linux_window_handle_from_ptr(self.handle)
                    .ok_or(raw_window_handle::HandleError::Unavailable)?;
                match handle {
                    LinuxWindowHandle::Xcb { window, connection } => {
                        use raw_window_handle::{WindowHandle, XcbWindowHandle};
                        if window == 0 || connection.is_null() {
                            return Err(raw_window_handle::HandleError::Unavailable);
                        }
                        unsafe {
                            Ok(WindowHandle::borrow_raw(
                                XcbWindowHandle::new(window, connection).into(),
                            ))
                        }
                    }
                    LinuxWindowHandle::Wayland { surface, .. } => {
                        use raw_window_handle::{WaylandWindowHandle, WindowHandle};
                        if surface.is_null() {
                            return Err(raw_window_handle::HandleError::Unavailable);
                        }
                        unsafe {
                            Ok(WindowHandle::borrow_raw(
                                WaylandWindowHandle::new(surface).into(),
                            ))
                        }
                    }
                }
            }
            #[cfg(target_os = "ios")]
            crate::platform::Platform::Mobile(crate::platform::MobilePlatform::IOS) => {
                use raw_window_handle::{UiKitWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: self.handle is a valid UIView from window creation
                let view = NonNull::new(self.handle as *mut std::ffi::c_void)
                    .ok_or(raw_window_handle::HandleError::Unavailable)?;
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        UiKitWindowHandle::new(view).into(),
                    ))
                }
            }
            #[cfg(target_os = "android")]
            crate::platform::Platform::Mobile(crate::platform::MobilePlatform::Android) => {
                use raw_window_handle::{AndroidNdkWindowHandle, WindowHandle};
                use std::ptr::NonNull;
                // SAFETY: self.handle is a valid Android window from window creation
                let window = NonNull::new(self.handle as *mut std::ffi::c_void)
                    .ok_or(raw_window_handle::HandleError::Unavailable)?;
                unsafe {
                    Ok(WindowHandle::borrow_raw(
                        AndroidNdkWindowHandle::new(window).into(),
                    ))
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
                unsafe { Ok(DisplayHandle::borrow_raw(AppKitDisplayHandle::new().into())) }
            }
            #[cfg(target_os = "windows")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::Windows) => {
                use raw_window_handle::{DisplayHandle, Win32DisplayHandle};
                unsafe { Ok(DisplayHandle::borrow_raw(Win32DisplayHandle::new().into())) }
            }
            #[cfg(target_os = "linux")]
            crate::platform::Platform::Desktop(crate::platform::DesktopPlatform::Linux) => {
                let handle = linux_window_handle_from_ptr(self.handle)
                    .ok_or(raw_window_handle::HandleError::Unavailable)?;
                match handle {
                    LinuxWindowHandle::Xcb { connection, .. } => {
                        use raw_window_handle::{DisplayHandle, XcbDisplayHandle};
                        if connection.is_null() {
                            return Err(raw_window_handle::HandleError::Unavailable);
                        }
                        unsafe {
                            Ok(DisplayHandle::borrow_raw(
                                XcbDisplayHandle::new(connection, 0).into(),
                            ))
                        }
                    }
                    LinuxWindowHandle::Wayland { display, .. } => {
                        use raw_window_handle::{DisplayHandle, WaylandDisplayHandle};
                        if display.is_null() {
                            return Err(raw_window_handle::HandleError::Unavailable);
                        }
                        unsafe {
                            Ok(DisplayHandle::borrow_raw(
                                WaylandDisplayHandle::new(display).into(),
                            ))
                        }
                    }
                }
            }
            #[cfg(target_os = "ios")]
            crate::platform::Platform::Mobile(crate::platform::MobilePlatform::IOS) => {
                use raw_window_handle::{DisplayHandle, UiKitDisplayHandle};
                unsafe { Ok(DisplayHandle::borrow_raw(UiKitDisplayHandle::new().into())) }
            }
            #[cfg(target_os = "android")]
            crate::platform::Platform::Mobile(crate::platform::MobilePlatform::Android) => {
                use raw_window_handle::{AndroidNdkDisplayHandle, DisplayHandle};
                unsafe {
                    Ok(DisplayHandle::borrow_raw(
                        AndroidNdkDisplayHandle::new().into(),
                    ))
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
    ///
    /// # Safety
    ///
    /// The window handle is valid for the lifetime of the window. We extend the
    /// lifetime to `'static` because the window is typically kept alive for the
    /// application lifetime, and wgpu surfaces are valid as long as the window exists.
    pub fn create_wgpu_surface(
        &self,
        instance: &wgpu::Instance,
    ) -> AureaResult<wgpu::Surface<'static>> {
        // Window implements HasWindowHandle and HasDisplayHandle (via native_handle)
        // wgpu's SurfaceTarget::from can create a surface target from such types
        let surface_target: wgpu::SurfaceTarget<'static> =
            unsafe { std::mem::transmute(wgpu::SurfaceTarget::from(self)) };

        let surface = instance
            .create_surface(surface_target)
            .map_err(|_| AureaError::ElementOperationFailed)?;

        Ok(surface)
    }
}
