use super::Window;
use crate::render::{ZenGpuContext, ZenGpuRenderer};
use crate::{AureaError, AureaResult};
use std::sync::Arc;

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
        let (pw, ph) = self.size();
        let lw = ((pw as f32 / scale).round() as u32).max(1);
        let lh = ((ph as f32 / scale).round() as u32).max(1);
        ZenGpuRenderer::with_context(&handles, context, lw, lh, scale)
    }

    /// Native window/display handles for building a `zengpu_hal` surface directly.
    pub fn zengpu_handles(&self) -> AureaResult<zengpu_hal::WindowHandles> {
        let native = crate::platform::handles::native_handle_from_window_ptr(self.handle)
            .ok_or(AureaError::ElementOperationFailed)?;
        crate::platform::zengpu::window_handles(&native)
    }
}
