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
//! let surface_target = SurfaceTarget::from(&native_handle);
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
use std::sync::Arc;
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
    /// dropped is undefined behaviour. If you need a `Surface<'static>` — to
    /// store it in a struct, or hand it to something that outlives this scope —
    /// put the window in an [`Arc`] and use
    /// [`create_wgpu_surface_owned`](Self::create_wgpu_surface_owned), which
    /// keeps the window alive for as long as the surface exists.
    pub fn create_wgpu_surface<'window>(
        &'window self,
        instance: &Instance,
    ) -> AureaResult<Surface<'window>> {
        // Window implements HasWindowHandle and HasDisplayHandle (via native_handle)
        // wgpu's SurfaceTarget::from can create a surface target from such types
        let surface_target = SurfaceTarget::from(self);

        let surface = instance
            .create_surface(surface_target)
            .map_err(|_| AureaError::ElementOperationFailed)?;

        Ok(surface)
    }

    /// Creates a wgpu surface that keeps the window alive.
    ///
    /// The returned surface is `'static` because it owns a clone of the `Arc`:
    /// the window cannot be dropped while the surface exists, which is exactly
    /// the invariant a `'static` surface needs.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    /// use std::sync::Arc;
    /// use wgpu::Instance;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Arc::new(Window::new("App", 800, 600)?);
    /// let instance = Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    /// let surface = Window::create_wgpu_surface_owned(&window, &instance)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_wgpu_surface_owned(
        window: &Arc<Self>,
        instance: &Instance,
    ) -> AureaResult<Surface<'static>> {
        let surface_target = SurfaceTarget::from(Arc::clone(window));

        instance
            .create_surface(surface_target)
            .map_err(|_| AureaError::ElementOperationFailed)
    }
}
