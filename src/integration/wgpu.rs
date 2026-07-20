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
//! let instance = Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
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
//! let instance = Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
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
use crate::platform::handles::NativeWindowHandle;
#[cfg(all(feature = "wgpu", target_os = "linux"))]
use crate::platform::handles::{LinuxWindowHandle, linux_window_handle_from_ptr};
#[cfg(feature = "wgpu")]
use crate::render::Canvas;
#[cfg(feature = "wgpu")]
use crate::view::FrameScheduler;
#[cfg(feature = "wgpu")]
use crate::window::{Window, WindowEvent, push_window_event};
#[cfg(feature = "wgpu")]
use crate::{AureaError, AureaResult};
#[cfg(feature = "wgpu")]
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
#[cfg(feature = "wgpu")]
use std::mem::transmute;

#[cfg(feature = "wgpu")]
impl HasWindowHandle for NativeWindowHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let (window, _display) = crate::platform::handles::raw_handles(self)?;
        // SAFETY: the raw handle was built from a native pointer that outlives
        // this borrow, per NativeWindowHandle's own safety contract.
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(window)) }
    }
}

#[cfg(feature = "wgpu")]
impl HasDisplayHandle for NativeWindowHandle {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let (_window, display) = crate::platform::handles::raw_handles(self)?;
        // SAFETY: the raw handle was built from a native pointer that outlives
        // this borrow, per NativeWindowHandle's own safety contract.
        unsafe { Ok(raw_window_handle::DisplayHandle::borrow_raw(display)) }
    }
}

/// Trait for Window to provide native handle implementation
///
/// This trait is used internally to implement `Window::native_handle()`.
#[cfg(feature = "wgpu")]
pub trait WindowNativeHandle {
    fn native_handle_impl(&self) -> NativeWindowHandle;
}

#[cfg(feature = "wgpu")]
impl WindowNativeHandle for Window {
    fn native_handle_impl(&self) -> NativeWindowHandle {
        #[cfg(target_os = "macos")]
        {
            let view_ptr = unsafe { ng_platform_window_get_content_view(self.handle) };
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
                    connection: null_mut(),
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

/// Classify a `CurrentSurfaceTexture` result for the given handle.
/// Pushes `WindowEvent::SurfaceLost` on Lost/Outdated and returns the action.
#[cfg(feature = "wgpu")]
pub fn handle_surface_result_for_handle(
    handle: *mut c_void,
    result: &wgpu::CurrentSurfaceTexture,
) -> SurfaceErrorAction {
    match result {
        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
            if !handle.is_null() {
                push_window_event(handle, WindowEvent::SurfaceLost);
                FrameScheduler::schedule();
            }
            SurfaceErrorAction::Recreate
        }
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
            SurfaceErrorAction::Skip
        }
        wgpu::CurrentSurfaceTexture::Validation => SurfaceErrorAction::Fatal,
        wgpu::CurrentSurfaceTexture::Success(_) | wgpu::CurrentSurfaceTexture::Suboptimal(_) => {
            SurfaceErrorAction::Skip
        }
    }
}

/// Call this when `Surface::get_current_texture()` returns a non-success result for a window.
/// Pushes `WindowEvent::SurfaceLost` on Lost/Outdated and returns Recreate/Skip/Fatal.
#[cfg(feature = "wgpu")]
pub fn handle_surface_result_for_window(
    window: &Window,
    result: &wgpu::CurrentSurfaceTexture,
) -> SurfaceErrorAction {
    handle_surface_result_for_handle(window.handle(), result)
}

/// Call this when `Surface::get_current_texture()` returns a non-success result for a canvas.
/// Pushes `WindowEvent::SurfaceLost` on Lost/Outdated and returns Recreate/Skip/Fatal.
#[cfg(feature = "wgpu")]
pub fn handle_surface_result_for_canvas(
    canvas: &Canvas,
    result: &wgpu::CurrentSurfaceTexture,
) -> SurfaceErrorAction {
    handle_surface_result_for_handle(canvas.window_handle(), result)
}

/// Call after recreating a window-backed wgpu surface so `SurfaceRecreated` is emitted and redraw scheduled.
#[cfg(feature = "wgpu")]
pub fn notify_surface_recreated_for_window(window: &Window) {
    push_window_event(window.handle(), WindowEvent::SurfaceRecreated);
    FrameScheduler::schedule();
}

#[cfg(feature = "wgpu")]
pub fn notify_surface_recreated_for_handle(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    push_window_event(handle, WindowEvent::SurfaceRecreated);
    FrameScheduler::schedule();
}

/// Call after recreating a canvas-backed wgpu surface so `SurfaceRecreated` is emitted and redraw scheduled.
#[cfg(feature = "wgpu")]
pub fn notify_surface_recreated_for_canvas(canvas: &Canvas) {
    let handle = canvas.window_handle();
    if handle.is_null() {
        return;
    }
    push_window_event(handle, WindowEvent::SurfaceRecreated);
    FrameScheduler::schedule();
}

#[cfg(feature = "wgpu")]
impl Window {
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
    /// let instance = Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
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
            unsafe { transmute(wgpu::SurfaceTarget::from(self)) };

        let surface = instance
            .create_surface(surface_target)
            .map_err(|_| AureaError::ElementOperationFailed)?;

        Ok(surface)
    }
}
