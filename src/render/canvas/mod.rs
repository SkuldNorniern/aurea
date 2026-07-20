use crate::elements::Element;
use crate::ffi::*;
use crate::registry::handle_key;
use crate::sync::lock;
use crate::view::{DamageRegion, FrameScheduler};
use crate::{AureaError, AureaResult};
use aurea_render::{
    ClickCallback, Color, CpuRasterizer, DrawingContext, HoverCallback,
    InteractionRegistry, InteractiveId, Point, Rect, Renderer, RendererBackend, Surface,
    SurfaceInfo,
};
use aurea_runtime::{FrameInfo, TickerId};
use std::collections::HashMap;
#[cfg(feature = "wgpu")]
use std::mem::transmute;
use std::os::raw::c_void;
use std::ptr::null_mut;
use std::sync::{Arc, LazyLock, Mutex};

mod runtime;

/// Drawing callback — Arc so it can be cheaply cloned out of the state lock before the renderer
/// lock is acquired, preventing deadlock when the callback reads canvas properties (size,
/// background_color, etc.).
pub type DrawCallback = Arc<dyn Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync>;

pub(crate) struct CanvasState {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub damage: DamageRegion,
    pub background_color: Color,
    pub draw_callback: Option<DrawCallback>,
    pub needs_redraw: bool,
    /// Physical-pixel damage rect from the previous rendered frame.
    pub prev_frame_damage: Option<Rect>,
}

static CANVAS_STATES: LazyLock<Mutex<HashMap<usize, Arc<Mutex<CanvasState>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn register_canvas_state(handle: usize, state: Arc<Mutex<CanvasState>>) {
    lock(&CANVAS_STATES).insert(handle, state);
}

fn unregister_canvas_state(handle: usize) {
    lock(&CANVAS_STATES).remove(&handle);
}

/// Request a full redraw of a canvas identified by its raw handle.
pub fn request_canvas_redraw(handle: usize) {
    let state = lock(&CANVAS_STATES).get(&handle).cloned();
    if let Some(state) = state {
        let mut st = lock(&state);
        st.damage.add_all();
        st.needs_redraw = true;
    }
    FrameScheduler::schedule_canvas(handle as *mut c_void);
    unsafe {
        ng_platform_canvas_invalidate(handle as *mut c_void);
    }
}

struct CanvasCleanup {
    handle: usize,
    renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
}

impl Drop for CanvasCleanup {
    fn drop(&mut self) {
        FrameScheduler::unregister_canvas(self.handle as *mut c_void);
        unregister_canvas_state(self.handle);
        let mut r = lock(&self.renderer);
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
    pub(crate) backend: RendererBackend,
    interaction_registry: Arc<InteractionRegistry>,
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
        let st = lock(&self.state);
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
    /// let instance = Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    /// let surface = canvas.create_wgpu_surface(&instance)?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "wgpu")]
    pub fn create_wgpu_surface(
        &self,
        instance: &wgpu::Instance,
    ) -> AureaResult<wgpu::Surface<'static>> {
        use crate::platform::handles::native_handle_from_canvas_ptr;

        let native_ptr = self.native_handle();
        let handle =
            native_handle_from_canvas_ptr(native_ptr).ok_or(AureaError::ElementOperationFailed)?;
        let surface_target: wgpu::SurfaceTarget<'static> =
            unsafe { transmute(wgpu::SurfaceTarget::from(&handle)) };
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

        let renderer = match backend {
            RendererBackend::Cpu => {
                let mut renderer: Box<dyn Renderer> = Box::new(CpuRasterizer::new(width, height));
                renderer.init(
                    Surface::OpenGL {
                        context: null_mut(),
                    },
                    SurfaceInfo {
                        width,
                        height,
                        scale_factor: 1.0,
                    },
                )?;
                Some(renderer)
            }
            #[cfg(feature = "zengpu")]
            RendererBackend::ZenGpu => {
                unsafe { ng_platform_canvas_set_gpu_owned(handle, 1) };
                None
            }
        };

        let scale_factor = unsafe {
            let window = ng_platform_canvas_get_window(handle);
            if !window.is_null() {
                ng_platform_get_scale_factor(window)
            } else {
                1.0
            }
        };

        let state = Arc::new(Mutex::new(CanvasState {
            width,
            height,
            scale_factor,
            damage: DamageRegion::new(16),
            background_color: Color::rgb(255, 255, 255),
            draw_callback: None,
            needs_redraw: false,
            prev_frame_damage: None,
        }));
        let renderer_arc = Arc::new(Mutex::new(renderer));
        let interaction_registry = Arc::new(InteractionRegistry::new());

        let canvas = Self {
            handle,
            state: state.clone(),
            renderer: renderer_arc.clone(),
            backend,
            interaction_registry,
            _cleanup: Arc::new(CanvasCleanup {
                handle: handle_key(handle),
                renderer: renderer_arc.clone(),
            }),
        };

        register_canvas_state(handle_key(handle), canvas.state.clone());
        canvas.register_with_scheduler(state, renderer_arc, backend);
        Ok(canvas)
    }

    /// Set the drawing callback (retained-mode style).
    pub fn set_draw_callback<F>(&self, callback: F) -> AureaResult<()>
    where
        F: Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync + 'static,
    {
        {
            let mut st = lock(&self.state);
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
        if lock(&self.renderer).is_none() {
            return Err(AureaError::ElementOperationFailed);
        }

        let (damage_rect, bg_color) = {
            let mut st = lock(&self.state);
            let damage = st.damage.take().or_else(|| {
                Some(super::Rect::new(
                    0.0,
                    0.0,
                    st.width as f32,
                    st.height as f32,
                ))
            });
            (damage, st.background_color)
        };

        {
            let mut r = lock(&self.renderer);
            if let Some(ref mut renderer) = *r {
                renderer.set_damage(damage_rect);
                let mut ctx = renderer.begin_frame()?;
                ctx.clear(bg_color)?;
                draw_fn(ctx.as_mut())?;
                renderer.end_frame()?;
            }
        }

        self.update_platform_view();
        unsafe {
            ng_platform_canvas_invalidate(self.handle);
        }
        Ok(())
    }

    /// Set background color.
    pub fn set_background_color(&self, color: Color) {
        let changed = {
            let mut st = lock(&self.state);
            if st.background_color == color {
                false
            } else {
                st.background_color = color;
                true
            }
        };
        if changed {
            self.invalidate_all();
        }
    }

    /// Get background color.
    pub fn background_color(&self) -> Color {
        lock(&self.state).background_color
    }

    /// Add damage to the canvas (called when content changes).
    pub fn add_damage(&self, rect: super::Rect) {
        lock(&self.state).damage.add(rect);
    }

    /// Mark the entire canvas as damaged and schedule a redraw.
    pub fn invalidate_all(&self) {
        {
            let mut st = lock(&self.state);
            st.damage.add_all();
            st.needs_redraw = true;
        }
        FrameScheduler::schedule_canvas(self.handle);
        unsafe {
            ng_platform_canvas_invalidate(self.handle);
        }
    }

    /// Check if canvas needs redraw and perform it.
    pub fn redraw_if_needed(&mut self) -> AureaResult<()> {
        let needs = {
            let mut st = lock(&self.state);
            if !st.needs_redraw {
                return Ok(());
            }
            st.needs_redraw = false;
            true
        };
        if needs {
            self.perform_redraw()?;
        }
        Ok(())
    }

    pub fn invalidate(&self) {
        self.invalidate_all();
    }

    /// Invalidate a specific rectangle.
    pub fn invalidate_rect(&self, rect: super::Rect) {
        {
            let mut st = lock(&self.state);
            st.damage.add(rect);
            st.needs_redraw = true;
        }
        FrameScheduler::schedule_canvas(self.handle);
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

    /// Start a per-frame ticker animation tied to this canvas.
    pub fn animate<F>(&self, ticker: F) -> TickerId
    where
        F: FnMut(FrameInfo) -> bool + Send + 'static,
    {
        let state = self.state.clone();
        let handle_usize = self.handle as usize;
        let mut user_ticker = ticker;

        FrameScheduler::register_ticker(move |info| {
            let keep = user_ticker(info);
            lock(&state).needs_redraw = true;
            FrameScheduler::schedule_canvas(handle_usize as *mut c_void);
            keep
        })
    }

    pub fn scale_factor(&self) -> f32 {
        lock(&self.state).scale_factor
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
        let r = lock(&self.renderer);
        if let Some(ref renderer) = *r
            && let Some(display_list) = renderer.display_list()
        {
            return self.interaction_registry.handle_click(display_list, point);
        }
        Ok(())
    }

    /// Handle a mouse hover event at the given coordinates.
    pub fn handle_hover(&self, x: f32, y: f32) -> AureaResult<()> {
        let sf = self.scale_factor();
        let point = Point::new(x * sf, y * sf);
        let r = lock(&self.renderer);
        if let Some(ref renderer) = *r
            && let Some(display_list) = renderer.display_list()
        {
            return self.interaction_registry.handle_hover(display_list, point);
        }
        Ok(())
    }
}

#[cfg(feature = "zengpu")]
pub(super) fn ensure_canvas_renderer(
    handle: *mut c_void,
    state: &Arc<Mutex<CanvasState>>,
    renderer: &Arc<Mutex<Option<Box<dyn Renderer>>>>,
    backend: RendererBackend,
) -> AureaResult<bool> {
    if lock(renderer).is_some() {
        return Ok(true);
    }
    if backend != RendererBackend::ZenGpu {
        return Ok(false);
    }

    let window = unsafe { ng_platform_canvas_get_window(handle) };
    if window.is_null() || unsafe { ng_platform_canvas_get_native_handle(handle) }.is_null() {
        return Ok(false);
    }

    let handles = zengpu_canvas_handles(handle)?;
    let (width, height, scale_factor) = {
        let st = lock(state);
        (st.width.max(1), st.height.max(1), st.scale_factor.max(1.0))
    };
    let gpu = aurea_render::ZenGpuRenderer::new(&handles, width, height, scale_factor)?;
    *lock(renderer) = Some(Box::new(gpu));
    Ok(true)
}

#[cfg(not(feature = "zengpu"))]
pub(super) fn ensure_canvas_renderer(
    _handle: *mut c_void,
    _state: &Arc<Mutex<CanvasState>>,
    renderer: &Arc<Mutex<Option<Box<dyn Renderer>>>>,
    _backend: RendererBackend,
) -> AureaResult<bool> {
    Ok(lock(renderer).is_some())
}

#[cfg(feature = "zengpu")]
fn zengpu_canvas_handles(handle: *mut c_void) -> AureaResult<zengpu_hal::WindowHandles> {
    #[cfg(target_os = "macos")]
    {
        let view = unsafe { ng_platform_canvas_get_native_handle(handle) };
        let native = crate::platform::handles::NativeWindowHandle::MacOS { ns_view: view };
        crate::platform::zengpu::window_handles(&native)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let native = crate::platform::handles::native_handle_from_canvas_ptr(handle)
            .ok_or(AureaError::ElementOperationFailed)?;
        crate::platform::zengpu::window_handles(&native)
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

#[cfg(all(test, feature = "zengpu"))]
mod tests {
    use super::*;

    #[test]
    fn zengpu_renderer_waits_for_canvas_attachment() {
        let canvas = Canvas::new(64, 64, RendererBackend::ZenGpu).unwrap();

        assert!(lock(&canvas.renderer).is_none());
        assert!(
            !ensure_canvas_renderer(
                canvas.handle,
                &canvas.state,
                &canvas.renderer,
                canvas.backend,
            )
            .unwrap()
        );
        assert!(lock(&canvas.renderer).is_none());
    }
}
