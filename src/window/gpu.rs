use super::Window;
use crate::platform::handles::native_handle_from_window_ptr;
use crate::platform::zengpu::window_handles;
use crate::render::{ZenGpuContext, ZenGpuRenderer};
use crate::{AureaError, AureaResult};
use std::sync::Arc;
use zengpu_hal::WindowHandles;

impl Window {
    /// Create a ZenGPU 2D renderer that presents directly to this window.
    ///
    /// This is aurea's window-level GPU path: the swapchain belongs to the
    /// window (one per window, not per widget). Drive the returned renderer
    /// like any [`Renderer`](aurea_render::Renderer) — `begin_frame` to record
    /// draws, `end_frame` to lower them to GPU draws and present.
    ///
    /// Requires the `zengpu` feature. Supported on Windows, macOS, and Linux
    /// (XCB or Wayland).
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
    ///
    /// Use this for editor/game integration where Aurea UI, offscreen game
    /// viewports, and additional windows must use the same logical device.
    pub fn create_zengpu_2d_with_context(
        &self,
        context: Arc<ZenGpuContext>,
    ) -> AureaResult<ZenGpuRenderer> {
        let handles = self.zengpu_handles()?;
        // `size()` is physical pixels; the renderer wants logical size + scale
        // (it scales drawing coords back up to physical, matching the swapchain
        // extent), so convert here.
        let scale = self.scale_factor().max(1.0);
        let (pw, ph) = self.size();
        // Values are always non-negative (physical size / scale, both >= 0),
        // well within u32's range for any real window size.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let lw = ((pw as f32 / scale).round() as u32).max(1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let lh = ((ph as f32 / scale).round() as u32).max(1);
        ZenGpuRenderer::with_context(&handles, context, lw, lh, scale)
    }

    /// Native window/display handles for building a `zengpu_hal` surface directly.
    ///
    /// Use this to drive `zengpu`/`zengpu_hal`/`zengpu_vulkan` APIs by hand
    /// instead of [`create_zengpu_2d`](Self::create_zengpu_2d)'s managed 2D
    /// renderer, e.g. for a custom 3D pipeline hosted in an Aurea window.
    pub fn zengpu_handles(&self) -> AureaResult<WindowHandles> {
        let native =
            native_handle_from_window_ptr(self.handle).ok_or(AureaError::ElementOperationFailed)?;
        window_handles(&native)
    }
}
