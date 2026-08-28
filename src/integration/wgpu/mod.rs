//! WebGPU/wgpu integration for external renderers.

mod surface_error;

pub use surface_error::{
    SurfaceErrorAction, handle_surface_result_for_canvas, handle_surface_result_for_handle,
    handle_surface_result_for_window, notify_surface_recreated_for_canvas,
    notify_surface_recreated_for_handle, notify_surface_recreated_for_window,
};

#[cfg(feature = "wgpu")]
use crate::platform::handles::NativeWindowHandle;
#[cfg(all(feature = "wgpu", target_os = "linux"))]
use crate::platform::handles::{LinuxWindowHandle, linux_window_handle_from_ptr};
#[cfg(feature = "wgpu")]
use crate::window::Window;
#[cfg(feature = "wgpu")]
use crate::{AureaError, AureaResult};
#[cfg(feature = "wgpu")]
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
#[cfg(feature = "wgpu")]
use std::sync::Arc;

#[cfg(feature = "wgpu")]
impl HasWindowHandle for NativeWindowHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let (window, _display) = crate::platform::handles::raw_handles(self)?;
        // SAFETY: the raw handle was built from a native pointer that outlives
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
        unsafe { Ok(raw_window_handle::DisplayHandle::borrow_raw(display)) }
    }
}

/// Trait for Window to provide native handle implementation.
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
    /// Create a wgpu surface from this window.
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
    pub fn create_wgpu_surface<'window>(
        &'window self,
        instance: &wgpu::Instance,
    ) -> AureaResult<wgpu::Surface<'window>> {
        let surface_target = wgpu::SurfaceTarget::from(self);

        let surface = instance
            .create_surface(surface_target)
            .map_err(|_| AureaError::ElementOperationFailed)?;

        Ok(surface)
    }

    /// Creates a wgpu surface that keeps the window alive.
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
        instance: &wgpu::Instance,
    ) -> AureaResult<wgpu::Surface<'static>> {
        let surface_target = wgpu::SurfaceTarget::from(Arc::clone(window));

        instance
            .create_surface(surface_target)
            .map_err(|_| AureaError::ElementOperationFailed)
    }
}
