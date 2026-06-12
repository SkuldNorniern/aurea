use crate::elements::Element;
use crate::ffi::*;
use crate::view::{DamageRegion, FrameScheduler};
use crate::{AureaError, AureaResult};
use aurea_foundation::CapabilityChecker;
use aurea_foundation::Platform;
use aurea_render::{
    CURRENT_BUFFER, ClickCallback, Color, CpuRasterizer, DrawingContext, GpuRasterizer,
    HoverCallback, InteractionRegistry, InteractiveId, Point, Renderer, RendererBackend, Surface,
    SurfaceInfo,
};
use std::os::raw::c_void;
use std::sync::{Arc, Mutex};

mod runtime;

/// Drawing callback — Arc so it can be cheaply cloned out of the state lock before the renderer
/// lock is acquired, preventing deadlock when the callback reads canvas properties (size,
/// background_color, etc.).
pub type DrawCallback =
    Arc<dyn Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync>;

pub(crate) struct CanvasState {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub damage: DamageRegion,
    pub background_color: Color,
    pub draw_callback: Option<DrawCallback>,
    pub needs_redraw: bool,
}

struct CanvasCleanup {
    handle: usize,
    renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
}

impl Drop for CanvasCleanup {
    fn drop(&mut self) {
        FrameScheduler::unregister_canvas(self.handle as *mut c_void);
        let mut r = crate::sync::lock(&self.renderer);
        if let Some(ref mut renderer) = *r {
            renderer.cleanup();
        }
    }
}

/// A drawable canvas element backed by a renderer.
#[derive(Clone)]
pub struct Canvas {
    pub(crate) handle: *mut c_void,
    pub(crate) state: Arc<Mutex<CanvasState>>,
    pub(crate) renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
    _backend: RendererBackend,
    interaction_registry: Arc<InteractionRegistry>,
    #[allow(dead_code)]
    platform: Platform,
    #[allow(dead_code)]
    capabilities: CapabilityChecker,
    _cleanup: Arc<CanvasCleanup>,
}

impl Canvas {
    /// Get the native window handle for this canvas.
    pub fn native_handle(&self) -> *mut c_void {
        unsafe { ng_platform_canvas_get_native_handle(self.handle) }
    }

    /// Get the parent window handle for this canvas.
    pub fn window_handle(&self) -> *mut c_void {
        unsafe { ng_platform_canvas_get_window(self.handle) }
    }

    /// Get canvas dimensions.
    pub fn size(&self) -> (u32, u32) {
        let st = crate::sync::lock(&self.state);
        (st.width, st.height)
    }

    /// Create a wgpu surface from this canvas.
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

        let native_ptr = self.native_handle();
        let handle =
            native_handle_from_canvas_ptr(native_ptr).ok_or(AureaError::ElementOperationFailed)?;
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
        let surface_info = SurfaceInfo { width, height, scale_factor: 1.0 };
        renderer.init(surface, surface_info)?;

        let platform = Platform::current();
        let capabilities = CapabilityChecker::new();
        let scale_factor = unsafe {
            let window = ng_platform_canvas_get_window(handle);
            if !window.is_null() { ng_platform_get_scale_factor(window) } else { 1.0 }
        };

        let state = Arc::new(Mutex::new(CanvasState {
            width,
            height,
            scale_factor,
            damage: DamageRegion::new(16),
            background_color: Color::rgb(255, 255, 255),
            draw_callback: None,
            needs_redraw: false,
        }));
        let renderer_arc = Arc::new(Mutex::new(Some(renderer)));
        let interaction_registry = Arc::new(InteractionRegistry::new());

        let canvas = Self {
            handle,
            state: state.clone(),
            renderer: renderer_arc.clone(),
            _backend: backend,
            interaction_registry,
            platform,
            capabilities,
            _cleanup: Arc::new(CanvasCleanup {
                handle: handle as usize,
                renderer: renderer_arc.clone(),
            }),
        };

        canvas.register_with_scheduler(state, renderer_arc);
        Ok(canvas)
    }

    /// Set the drawing callback (retained-mode style).
    pub fn set_draw_callback<F>(&self, callback: F) -> AureaResult<()>
    where
        F: Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync + 'static,
    {
        {
            let mut st = crate::sync::lock(&self.state);
            st.draw_callback = Some(Arc::new(callback));
            st.needs_redraw = true;
        }
        self.invalidate_all();
        Ok(())
    }

    /// Draw immediately (legacy API - still supported).
    pub fn draw<F>(&mut self, draw_fn: F) -> AureaResult<()>
    where
        F: FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    {
        self.check_and_resize()?;

        let (damage_rect, bg_color) = {
            let mut st = crate::sync::lock(&self.state);
            let damage = st.damage.take().or_else(|| {
                Some(super::Rect::new(0.0, 0.0, st.width as f32, st.height as f32))
            });
            (damage, st.background_color)
        };

        {
            let mut r = crate::sync::lock(&self.renderer);
            if let Some(ref mut renderer) = *r {
                renderer.set_damage(damage_rect);
                let mut ctx = renderer.begin_frame()?;
                ctx.clear(bg_color)?;
                draw_fn(ctx.as_mut())?;
                renderer.end_frame()?;
            }
        }

        self.update_platform_view();
        unsafe { ng_platform_canvas_invalidate(self.handle); }
        Ok(())
    }

    /// Set background color.
    pub fn set_background_color(&self, color: Color) {
        let changed = {
            let mut st = crate::sync::lock(&self.state);
            if st.background_color == color { false } else { st.background_color = color; true }
        };
        if changed { self.invalidate_all(); }
    }

    /// Get background color.
    pub fn background_color(&self) -> Color {
        crate::sync::lock(&self.state).background_color
    }

    /// Add damage to the canvas (called when content changes).
    pub fn add_damage(&self, rect: super::Rect) {
        crate::sync::lock(&self.state).damage.add(rect);
    }

    /// Mark the entire canvas as damaged and schedule a redraw.
    pub fn invalidate_all(&self) {
        {
            let mut st = crate::sync::lock(&self.state);
            st.damage.add_all();
            st.needs_redraw = true;
        }
        FrameScheduler::schedule_canvas(self.handle);
        unsafe { ng_platform_canvas_invalidate(self.handle); }
    }

    /// Check if canvas needs redraw and perform it.
    pub fn redraw_if_needed(&mut self) -> AureaResult<()> {
        let needs = {
            let mut st = crate::sync::lock(&self.state);
            if !st.needs_redraw { return Ok(()); }
            st.needs_redraw = false;
            true
        };
        if needs { self.perform_redraw()?; }
        Ok(())
    }

    pub fn invalidate(&self) {
        self.invalidate_all();
    }

    /// Invalidate a specific rectangle.
    pub fn invalidate_rect(&self, rect: super::Rect) {
        {
            let mut st = crate::sync::lock(&self.state);
            st.damage.add(rect);
            st.needs_redraw = true;
        }
        FrameScheduler::schedule_canvas(self.handle);
        unsafe {
            ng_platform_canvas_invalidate_rect(self.handle, rect.x, rect.y, rect.width, rect.height);
        }
    }

    pub fn width(&self) -> u32 { self.size().0 }
    pub fn height(&self) -> u32 { self.size().1 }

    /// Start a per-frame ticker animation tied to this canvas.
    pub fn animate<F>(&self, ticker: F) -> aurea_runtime::TickerId
    where
        F: FnMut(aurea_runtime::FrameInfo) -> bool + Send + 'static,
    {
        let state = self.state.clone();
        let handle_usize = self.handle as usize;
        let mut user_ticker = ticker;

        aurea_runtime::FrameScheduler::register_ticker(move |info| {
            let keep = user_ticker(info);
            crate::sync::lock(&state).needs_redraw = true;
            aurea_runtime::FrameScheduler::schedule_canvas(handle_usize as *mut c_void);
            keep
        })
    }

    pub fn scale_factor(&self) -> f32 {
        crate::sync::lock(&self.state).scale_factor
    }

    /// Register a click callback for an interactive shape.
    pub fn on_click(&self, id: InteractiveId, callback: ClickCallback) -> AureaResult<()> {
        self.interaction_registry.register_click(id, callback);
        Ok(())
    }

    /// Register a hover callback for an interactive shape.
    pub fn on_hover(&self, id: InteractiveId, callback: HoverCallback) -> AureaResult<()> {
        self.interaction_registry.register_hover(id, callback);
        Ok(())
    }

    /// Handle a mouse/touch click event at the given coordinates.
    pub fn handle_click(&self, x: f32, y: f32) -> AureaResult<()> {
        let sf = self.scale_factor();
        let point = Point::new(x * sf, y * sf);
        let r = crate::sync::lock(&self.renderer);
        if let Some(ref renderer) = *r {
            if let Some(display_list) = renderer.display_list() {
                return self.interaction_registry.handle_click(display_list, point);
            }
        }
        Ok(())
    }

    /// Handle a mouse hover event at the given coordinates.
    pub fn handle_hover(&self, x: f32, y: f32) -> AureaResult<()> {
        let sf = self.scale_factor();
        let point = Point::new(x * sf, y * sf);
        let r = crate::sync::lock(&self.renderer);
        if let Some(ref renderer) = *r {
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

