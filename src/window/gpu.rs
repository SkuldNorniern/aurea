use super::Window;
#[cfg(target_os = "macos")]
use crate::ffi::ng_macos_window_use_metal_layer;
use crate::platform::handles::native_handle_from_window_ptr;
use crate::platform::zengpu::window_handles;
use crate::render::{ZenGpuContext, ZenGpuRenderer};
use crate::{AureaError, AureaResult};
use std::sync::Arc;
use zengpu_hal::WindowHandles;

impl Window {
    /// Create a ZenGPU 2D renderer that presents directly to this window.
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "zengpu")]
    /// # fn demo() -> Result<(), Box<dyn std::error::Error>> {
    /// use aurea::Window;
    /// let window = Window::new("GPU", 800, 600)?;
    /// let mut renderer = window.create_zengpu_2d()?;
    /// # let _ = &mut renderer;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_zengpu_2d(&self) -> AureaResult<ZenGpuRenderer> {
        self.create_zengpu_2d_with_context(Arc::new(ZenGpuContext::new()?))
    }

    /// Create a ZenGPU 2D renderer on a caller-owned shared GPU context.
    pub fn create_zengpu_2d_with_context(
        &self,
        context: Arc<ZenGpuContext>,
    ) -> AureaResult<ZenGpuRenderer> {
        let handles = self.zengpu_handles()?;
        let scale = self.scale_factor().max(1.0);
        let (lw, lh) = self.size();
        ZenGpuRenderer::with_context(&handles, context, lw.max(1), lh.max(1), scale)
    }

    /// Native window/display handles for building a `zengpu_hal` surface directly.
    pub fn zengpu_handles(&self) -> AureaResult<WindowHandles> {
        #[cfg(target_os = "macos")]
        if unsafe { ng_macos_window_use_metal_layer(self.handle) } != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        let native =
            native_handle_from_window_ptr(self.handle).ok_or(AureaError::ElementOperationFailed)?;
        window_handles(&native)
    }
}
