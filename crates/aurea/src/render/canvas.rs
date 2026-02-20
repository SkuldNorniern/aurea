use aurea_render::{
    ClickCallback, Color, CpuRasterizer, CURRENT_BUFFER, DrawingContext, GpuRasterizer,
    HoverCallback, InteractionRegistry, InteractiveId, Point, Renderer, RendererBackend,
    Surface, SurfaceInfo,
};
use aurea_core::CapabilityChecker;
use crate::elements::Element;
use crate::ffi::*;
use aurea_core::Platform;
use crate::view::{DamageRegion, FrameScheduler};
use crate::{AureaError, AureaResult};
use std::os::raw::c_void;
use std::sync::{Arc, Mutex};

mod runtime;

/// Drawing callback type - stored and called when canvas needs redraw
pub type DrawCallback = Box<dyn Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync>;

#[derive(Debug, Clone, Copy)]
struct CanvasMetrics {
    width: u32,
    height: u32,
    scale_factor: f32,
}

/// A drawable canvas element backed by a renderer.
pub struct Canvas {
    handle: *mut c_void,
    renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
    _backend: RendererBackend,
    metrics: Arc<Mutex<CanvasMetrics>>,
    damage: Arc<Mutex<DamageRegion>>,
    // Retained-mode properties
    background_color: Arc<Mutex<Color>>,
    draw_callback: Arc<Mutex<Option<DrawCallback>>>,
    needs_redraw: Arc<Mutex<bool>>,
    // Interaction system
    interaction_registry: Arc<InteractionRegistry>,
    #[allow(dead_code)]
    platform: Platform,
    #[allow(dead_code)]
    capabilities: CapabilityChecker,
}

impl Canvas {
    /// Get the native window handle for this canvas
    ///
    /// This can be used to create platform-specific surfaces (e.g., WGPU, Vulkan, Metal).
    /// Returns a platform-specific handle:
    /// - macOS: CALayer or NSView pointer
    /// - Windows: HWND
    /// - Linux: X11 Window or Wayland Surface
    pub fn native_handle(&self) -> *mut c_void {
        unsafe { ng_platform_canvas_get_native_handle(self.handle) }
    }

    /// Get the parent window handle for this canvas
    pub fn window_handle(&self) -> *mut c_void {
        unsafe { ng_platform_canvas_get_window(self.handle) }
    }

    /// Get canvas dimensions
    pub fn size(&self) -> (u32, u32) {
        let metrics = crate::sync::lock(self.metrics.as_ref());
        (metrics.width, metrics.height)
    }

    /// Create a wgpu surface from this canvas
    ///
    /// This creates a wgpu surface for 3D rendering within the canvas.
    /// Similar to Window::create_wgpu_surface() but for Canvas widgets.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::render::{Canvas, RendererBackend};
    /// use wgpu::Instance;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let canvas = Canvas::new(800, 600, RendererBackend::Cpu)?;
    /// let instance = Instance::new(wgpu::InstanceDescriptor::default());
    /// let surface = canvas.create_wgpu_surface(&instance)?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "wgpu")]
    pub fn create_wgpu_surface(
        &self,
        instance: &wgpu::Instance,
    ) -> AureaResult<wgpu::Surface<'static>> {
        use crate::integration::wgpu::native_handle_from_canvas_ptr;
        use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

        // Get the native handle for the canvas
        let native_ptr = self.native_handle();

        let handle =
            native_handle_from_canvas_ptr(native_ptr).ok_or(AureaError::ElementOperationFailed)?;

        // Create surface target from handle
        let surface_target: wgpu::SurfaceTarget<'static> =
            unsafe { std::mem::transmute(wgpu::SurfaceTarget::from(&handle)) };

        let surface = instance
            .create_surface(surface_target)
            .map_err(|_| AureaError::ElementOperationFailed)?;

        Ok(surface)
    }

    /// Create a new canvas with the given size and renderer backend.
    pub fn new(width: u32, height: u32, backend: RendererBackend) -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_canvas(width as i32, height as i32) };
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        let mut renderer: Box<dyn Renderer> = match backend {
            RendererBackend::Cpu => Box::new(CpuRasterizer::new(width, height)),
            RendererBackend::Gpu => Box::new(GpuRasterizer::new(width, height)),
        };
        let surface = Surface::OpenGL {
            context: std::ptr::null_mut(),
        };
        let surface_info = SurfaceInfo {
            width,
            height,
            scale_factor: 1.0,
        };

        renderer.init(surface, surface_info)?;

        let platform = Platform::current();
        let capabilities = CapabilityChecker::new();

        let scale_factor = unsafe {
            let window = ng_platform_canvas_get_window(handle);
            if !window.is_null() {
                ng_platform_get_scale_factor(window)
            } else {
                1.0
            }
        };

        let renderer_arc = Arc::new(Mutex::new(Some(renderer)));
        let metrics = Arc::new(Mutex::new(CanvasMetrics {
            width,
            height,
            scale_factor,
        }));
        let damage_arc = Arc::new(Mutex::new(DamageRegion::new(16)));
        let bg_color_arc = Arc::new(Mutex::new(Color::rgb(255, 255, 255)));
        let draw_cb_arc = Arc::new(Mutex::new(None));
        let needs_redraw_arc = Arc::new(Mutex::new(false));
        let interaction_registry = Arc::new(InteractionRegistry::new());

        // Clone Arcs for callback
        let renderer_for_callback = renderer_arc.clone();
        let damage_for_callback = damage_arc.clone();
        let bg_color_for_callback = bg_color_arc.clone();
        let draw_cb_for_callback = draw_cb_arc.clone();
        let needs_redraw_for_callback = needs_redraw_arc.clone();

        let canvas = Self {
            handle,
            renderer: renderer_arc,
            _backend: backend,
            metrics: metrics.clone(),
            damage: damage_arc,
            background_color: bg_color_arc,
            draw_callback: draw_cb_arc,
            needs_redraw: needs_redraw_arc,
            interaction_registry,
            platform,
            capabilities,
        };

        // Register with frame scheduler for automatic redraw
        canvas.register_with_scheduler(
            renderer_for_callback,
            metrics,
            damage_for_callback,
            bg_color_for_callback,
            draw_cb_for_callback,
            needs_redraw_for_callback,
        );

        Ok(canvas)
    }

    /// Set the drawing callback (retained-mode style)
    /// The callback will be called automatically when the canvas needs redraw
    pub fn set_draw_callback<F>(&self, callback: F) -> AureaResult<()>
    where
        F: Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync + 'static,
    {
        let mut cb = crate::sync::lock(self.draw_callback.as_ref());
        *cb = Some(Box::new(callback));

        // Mark as needing redraw
        *crate::sync::lock(self.needs_redraw.as_ref()) = true;
        self.invalidate_all();

        Ok(())
    }

    /// Draw immediately (legacy API - still supported)
    /// Prefer using `set_draw_callback()` for retained-mode style
    pub fn draw<F>(&mut self, draw_fn: F) -> AureaResult<()>
    where
        F: FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    {
        // For FnOnce, we can't store it, so we execute immediately
        // This maintains backward compatibility
        self.check_and_resize()?;

        {
            let mut renderer_guard = crate::sync::lock(self.renderer.as_ref());
            if let Some(ref mut renderer) = *renderer_guard {
                // Get damage region for this frame - use full canvas if empty
                let damage_rect = {
                    let mut damage = crate::sync::lock(self.damage.as_ref());
                    let rect = damage.take();
                    // If no damage region, use full canvas
                    rect.or_else(|| {
                        let (width, height) = self.size();
                        Some(super::Rect::new(0.0, 0.0, width as f32, height as f32))
                    })
                };

                // Set damage in renderer (for partial redraw support)
                renderer.set_damage(damage_rect);

                let mut ctx = renderer.begin_frame()?;

                // Clear with background color
                let bg_color = *crate::sync::lock(self.background_color.as_ref());
                ctx.clear(bg_color)?;

                // Execute the draw function
                draw_fn(ctx.as_mut())?;

                renderer.end_frame()?;
            }
        }

        // Update platform view after releasing renderer lock
        self.update_platform_view();

        // Invalidate platform view to trigger redraw
        unsafe {
            ng_platform_canvas_invalidate(self.handle);
        }

        Ok(())
    }

    /// Set background color (property setter - marks damage automatically)
    pub fn set_background_color(&self, color: Color) {
        let mut bg = crate::sync::lock(self.background_color.as_ref());
        if *bg != color {
            *bg = color;
            self.invalidate_all();
        }
    }

    /// Get background color
    pub fn background_color(&self) -> Color {
        *crate::sync::lock(self.background_color.as_ref())
    }

    /// Add damage to the canvas (called when content changes)
    pub fn add_damage(&self, rect: super::Rect) {
        let mut damage = crate::sync::lock(self.damage.as_ref());
        damage.add(rect);
    }

    /// Mark the entire canvas as damaged
    /// This automatically schedules a redraw using the stored callback
    pub fn invalidate_all(&self) {
        let mut damage = crate::sync::lock(self.damage.as_ref());
        damage.add_all();

        // Schedule frame for redraw
        FrameScheduler::schedule();

        // Mark that we need to redraw
        *crate::sync::lock(self.needs_redraw.as_ref()) = true;

        // Invalidate platform view
        unsafe {
            ng_platform_canvas_invalidate(self.handle);
        }
    }

    /// Check if canvas needs redraw and perform it
    /// This should be called from the frame scheduler or window's frame handler
    pub fn redraw_if_needed(&mut self) -> AureaResult<()> {
        let needs_redraw = {
            let mut flag = crate::sync::lock(self.needs_redraw.as_ref());
            if !*flag {
                return Ok(());
            }
            *flag = false;
            true
        };

        if needs_redraw {
            self.perform_redraw()?;
        }

        Ok(())
    }

    pub fn invalidate(&self) {
        self.invalidate_all();
    }

    /// Invalidate a specific rectangle (property setter style - marks damage automatically)
    pub fn invalidate_rect(&self, rect: super::Rect) {
        // Add to damage region
        self.add_damage(rect);

        // Schedule frame
        FrameScheduler::schedule();

        // Mark needs redraw
        *crate::sync::lock(self.needs_redraw.as_ref()) = true;

        // Invalidate platform view
        unsafe {
            ng_platform_canvas_invalidate_rect(
                self.handle,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
            );
        }
    }

    pub fn width(&self) -> u32 {
        self.size().0
    }

    pub fn height(&self) -> u32 {
        self.size().1
    }

    pub fn scale_factor(&self) -> f32 {
        let metrics = crate::sync::lock(self.metrics.as_ref());
        metrics.scale_factor
    }

    /// Register a click callback for an interactive shape
    pub fn on_click(&self, id: InteractiveId, callback: ClickCallback) -> AureaResult<()> {
        self.interaction_registry.register_click(id, callback);
        Ok(())
    }

    /// Register a hover callback for an interactive shape
    pub fn on_hover(&self, id: InteractiveId, callback: HoverCallback) -> AureaResult<()> {
        self.interaction_registry.register_hover(id, callback);
        Ok(())
    }

    /// Handle a mouse/touch click event at the given coordinates
    /// This should be called from platform event handlers
    pub fn handle_click(&self, x: f32, y: f32) -> AureaResult<()> {
        let point = Point::new(x, y);

        // Get display list from renderer
        let renderer_guard = crate::sync::lock(self.renderer.as_ref());
        if let Some(ref renderer) = *renderer_guard {
            if let Some(display_list) = renderer.display_list() {
                return self.interaction_registry.handle_click(display_list, point);
            }
        }

        Ok(())
    }

    /// Handle a mouse hover event at the given coordinates
    /// This should be called from platform event handlers
    pub fn handle_hover(&self, x: f32, y: f32) -> AureaResult<()> {
        let point = Point::new(x, y);

        let renderer_guard = crate::sync::lock(self.renderer.as_ref());
        if let Some(ref renderer) = *renderer_guard {
            if let Some(display_list) = renderer.display_list() {
                return self.interaction_registry.handle_hover(display_list, point);
            }
        }

        Ok(())
    }

}

impl Element for Canvas {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, rect: Option<super::Rect>) {
        if let Some(r) = rect {
            unsafe {
                ng_platform_canvas_invalidate_rect(self.handle, r.x, r.y, r.width, r.height);
            }
        } else {
            unsafe {
                ng_platform_canvas_invalidate(self.handle);
            }
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // Unregister from frame scheduler
        FrameScheduler::unregister_canvas(self.handle);

        // Cleanup renderer
        let mut renderer_guard = crate::sync::lock(self.renderer.as_ref());
        if let Some(ref mut renderer) = *renderer_guard {
            renderer.cleanup();
        }
    }
}
