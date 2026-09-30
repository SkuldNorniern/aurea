//! WebGPU/wgpu integration for external renderers.

mod surface_error;

pub use surface_error::{
    SurfaceErrorAction, handle_surface_result_for_canvas, handle_surface_result_for_handle,
    handle_surface_result_for_window, notify_surface_recreated_for_canvas,
    notify_surface_recreated_for_handle, notify_surface_recreated_for_window,
};

#[cfg(all(feature = "wgpu", target_os = "linux"))]
use crate::platform::handles::linux_window_handle_from_ptr;
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
        unsafe { Ok(WindowHandle::borrow_raw(window)) }
    }
}

#[cfg(feature = "wgpu")]
impl HasDisplayHandle for NativeWindowHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let (_window, display) = raw_handles(self)?;
        // SAFETY: the raw handle was built from a native pointer that outlives
        unsafe { Ok(DisplayHandle::borrow_raw(display)) }
    }
}

/// Trait for Window to provide native handle implementation.
#[cfg(feature = "wgpu")]
pub trait WindowNativeHandle {
    /// `None` when the platform will not give one up — a window that is not realised yet, or a
    /// display server this build cannot read a surface from.
    fn native_handle_impl(&self) -> Option<NativeWindowHandle>;
}

#[cfg(feature = "wgpu")]
impl WindowNativeHandle for Window {
    fn native_handle_impl(&self) -> Option<NativeWindowHandle> {
        #[cfg(target_os = "macos")]
        {
            let view_ptr = unsafe { ng_platform_window_get_content_view(self.handle()) };
            if view_ptr.is_null() {
                return None;
            }
            Some(NativeWindowHandle::MacOS { ns_view: view_ptr })
        }
        #[cfg(target_os = "windows")]
        {
            Some(NativeWindowHandle::Windows {
                hwnd: self.handle(),
            })
        }
        #[cfg(target_os = "linux")]
        {
            linux_window_handle_from_ptr(self.handle()).map(NativeWindowHandle::Linux)
        }
        #[cfg(target_os = "ios")]
        {
            Some(NativeWindowHandle::IOS {
                ui_view: self.handle(),
            })
        }
        #[cfg(target_os = "android")]
        {
            Some(NativeWindowHandle::Android {
                native_window: self.handle(),
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
        instance: &Instance,
    ) -> AureaResult<Surface<'window>> {
        instance
            .create_surface(SurfaceTarget::from(self.surface_target()))
            .map_err(|_| AureaError::ElementOperationFailed)
    }
}
