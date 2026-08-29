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
//! # Reading the native handle
//!
//! [`Window::create_wgpu_surface`] is the way to get a surface. The handle
//! underneath is available too, for talking to a graphics API directly:
//!
//! ```rust,no_run
//! use aurea::Window;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let window = Window::new("App", 800, 600)?;
//! // `None` when the platform will not give one up — a window that is not
//! // realised yet, or a display server this build cannot read a surface from.
//! let Some(native_handle) = window.native_handle() else {
//!     return Ok(());
//! };
//! # let _ = native_handle;
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

mod surface_error;

pub use surface_error::{
    SurfaceErrorAction, handle_surface_result_for_canvas, handle_surface_result_for_handle,
    handle_surface_result_for_window, notify_surface_recreated_for_canvas,
    notify_surface_recreated_for_handle, notify_surface_recreated_for_window,
};

#[cfg(all(feature = "wgpu", target_os = "linux"))]
use crate::platform::handles::{LinuxWindowHandle, linux_window_handle_from_ptr};
#[cfg(feature = "wgpu")]
use crate::platform::handles::{NativeWindowHandle, raw_handles};
#[cfg(feature = "wgpu")]
use crate::window::Window;
#[cfg(feature = "wgpu")]
use crate::{AureaError, AureaResult};
#[cfg(feature = "wgpu")]
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
#[cfg(feature = "wgpu")]
#[cfg(feature = "wgpu")]
use wgpu::{Instance, Surface, SurfaceTarget};

#[cfg(feature = "wgpu")]
impl HasWindowHandle for NativeWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let (window, _display) = raw_handles(self)?;
        // SAFETY: the raw handle was built from a native pointer that outlives
        // this borrow, per NativeWindowHandle's own safety contract.
        unsafe { Ok(WindowHandle::borrow_raw(window)) }
    }
}

#[cfg(feature = "wgpu")]
impl HasDisplayHandle for NativeWindowHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let (_window, display) = raw_handles(self)?;
        // SAFETY: the raw handle was built from a native pointer that outlives
        // this borrow, per NativeWindowHandle's own safety contract.
        unsafe { Ok(DisplayHandle::borrow_raw(display)) }
    }
}

/// Trait for Window to provide native handle implementation
///
/// This trait is used internally to implement `Window::native_handle()`.
#[cfg(feature = "wgpu")]
pub trait WindowNativeHandle {
    /// `None` when the platform will not give one up — a window that is not
    /// realised yet, or a display server this build cannot read a surface
    /// from. Handing back a zeroed handle of the right shape instead only
    /// moved the failure into whatever tried to draw on it.
    fn native_handle_impl(&self) -> Option<NativeWindowHandle>;
}

#[cfg(feature = "wgpu")]
impl WindowNativeHandle for Window {
    fn native_handle_impl(&self) -> Option<NativeWindowHandle> {
        #[cfg(target_os = "macos")]
        {
            let view_ptr = unsafe { ng_platform_window_get_content_view(self.handle) };
            if view_ptr.is_null() {
                return None;
            }
            Some(NativeWindowHandle::MacOS { ns_view: view_ptr })
        }
        #[cfg(target_os = "windows")]
        {
            Some(NativeWindowHandle::Windows { hwnd: self.handle })
        }
        #[cfg(target_os = "linux")]
        {
            linux_window_handle_from_ptr(self.handle).map(NativeWindowHandle::Linux)
        }
        #[cfg(target_os = "ios")]
        {
            Some(NativeWindowHandle::IOS {
                ui_view: self.handle,
            })
        }
        #[cfg(target_os = "android")]
        {
            Some(NativeWindowHandle::Android {
                native_window: self.handle,
            })
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
    /// # Lifetime
    ///
    /// The surface borrows the window, because that is the truth: the native
    /// window backs the surface, and using the surface after the window is
    /// dropped is undefined behaviour. Keep the window in scope alongside it.
    pub fn create_wgpu_surface<'window>(
        &'window self,
        instance: &Instance,
    ) -> AureaResult<Surface<'window>> {
        instance
            .create_surface(SurfaceTarget::from(self.surface_target()))
            .map_err(|_| AureaError::ElementOperationFailed)
    }
}
