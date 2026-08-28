use crate::elements::Element;
use crate::ffi::*;
#[cfg(feature = "wgpu")]
use crate::platform::handles::{NativeWindowHandle, native_handle_from_canvas_ptr};
use crate::registry::handle_key;
use crate::{AureaError, AureaResult};
use aurea_foundation::lock;
use aurea_render::{
    ClickCallback, Color, CpuRasterizer, DrawingContext, HoverCallback, InteractionRegistry,
    InteractiveId, Point, Renderer, RendererBackend, Surface, SurfaceInfo,
};
use aurea_runtime::{DamageRegion, FrameScheduler};
use aurea_runtime::{FrameInfo, TickerId};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(feature = "wgpu")]
use wgpu::{Instance, Surface as WgpuSurface, SurfaceTarget};

mod runtime;
mod state;

pub use state::request_canvas_redraw;
use state::{CanvasCleanup, CanvasState, ensure_canvas_renderer, register_canvas_state};

/// Drawing callback — Arc so it can be cheaply cloned out of the state lock
/// before the renderer lock is acquired, preventing deadlock when the callback
/// reads canvas properties (size, background_color, etc.).
pub type DrawCallback = Arc<dyn Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync>;

/// A drawable canvas element backed by a renderer.
///
/// `Canvas` is cheaply cloneable: clones share the same native handle, state,
/// and renderer. This lets one clone be handed to the window as content while
/// another stays in the application loop for immediate-mode drawing:
///
/// ```rust,ignore
/// let canvas = Canvas::new(800, 600, RendererBackend::Cpu)?;
/// let mut draw_canvas = canvas.clone();
/// window.set_content(canvas)?;
/// loop {
///     draw_canvas.draw(|ctx| { /* … */ Ok(()) })?;
///     window.process_frames()?;
/// }
/// ```
///
/// Cleanup (scheduler unregister, renderer teardown) runs when the last clone
/// is dropped.
#[derive(Clone)]
pub struct Canvas {
    pub(crate) handle: *mut c_void,
    pub(crate) state: Arc<Mutex<CanvasState>>,
    pub(crate) renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
    pub(crate) backend: RendererBackend,
    interaction_registry: Arc<InteractionRegistry>,
    /// The canvas's native handle, kept so a wgpu surface can borrow something
    /// that genuinely lives as long as the canvas does.
    #[cfg(feature = "wgpu")]
    surface_handle: Arc<NativeWindowHandle>,
    _cleanup: Arc<CanvasCleanup>,
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
        let st = lock(&self.state);
        (st.width, st.height)
    }

    /// Create a wgpu surface from this canvas
    ///
    /// # Lifetime
    ///
    /// The surface borrows the canvas: the native canvas object backs the
    /// surface, and it is torn down when the last `Canvas` clone is dropped.
    /// There is deliberately no `'static` variant: a canvas is torn down by
    /// its own cleanup, so nothing the surface could hold would keep the
    /// native object alive. Keep the canvas in scope alongside the surface.
    ///
    /// # Example
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
    pub fn create_wgpu_surface<'canvas>(
        &'canvas self,
        instance: &Instance,
    ) -> AureaResult<WgpuSurface<'canvas>> {
        instance
            .create_surface(SurfaceTarget::from(&*self.surface_handle))
            .map_err(|_| AureaError::ElementOperationFailed)
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
                    Surface::Cpu,
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

        #[cfg(feature = "wgpu")]
        let surface_handle = Arc::new(
            native_handle_from_canvas_ptr(unsafe { ng_platform_canvas_get_native_handle(handle) })
                .ok_or(AureaError::ElementOperationFailed)?,
        );

        let canvas = Self {
            handle,
            state: state.clone(),
            renderer: renderer_arc.clone(),
            backend,
            interaction_registry,
            #[cfg(feature = "wgpu")]
            surface_handle,
            _cleanup: Arc::new(CanvasCleanup {
                handle: handle_key(handle),
                renderer: renderer_arc.clone(),
                owns_native: AtomicBool::new(true),
            }),
        };

        register_canvas_state(handle_key(handle), canvas.state.clone());
        canvas.register_with_scheduler(state, renderer_arc, backend);
        Ok(canvas)
    }

    /// Set the drawing callback (retained-mode style).
    /// The callback will be called automatically when the canvas needs redraw.
    ///
    /// # Idempotency contract
    ///
    /// The renderer's damage tracker (see `aurea-render`'s P6-A diff/tile
    /// cache) assumes that re-running this callback with unchanged
    /// application state issues the *same draw commands in the same order*
    /// as the previous frame, producing identical `cache_key`s. The
    /// scheduler already re-invokes this callback on every frame it decides
    /// to redraw, so a callback whose output depends on anything other than
    /// the application state it captures (e.g. wall-clock time, RNG, or
    /// iteration order over a `HashMap`) is already visibly broken today —
    /// it would flicker or jitter even without the tile cache. The tile
    /// cache does not introduce a new requirement, but it does make
    /// violations cheaper to miss: a non-deterministic callback can produce
    /// a display list that hashes the same as last frame's for some tiles
    /// and differently for others, redrawing only part of the scene.
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
    /// Prefer using `set_draw_callback()` for retained-mode style.
    ///
    /// # Damage tracking
    ///
    /// Each call always carries an "always-dirty" damage hint to the
    /// renderer: any region queued via `add_damage`/`invalidate_rect` since
    /// the last frame, or the *entire* canvas if nothing was queued. That
    /// hint is forced-dirty regardless of the tile cache's content hashes
    /// (see `CpuRasterizer::compute_dirty_tiles`'s `forced` parameter), so
    /// calling this repeatedly with identical content still repaints the
    /// hinted region every time rather than silently going stale.
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
                {
                    let mut ctx = renderer.begin_frame()?;
                    ctx.clear(bg_color)?;
                    draw_fn(ctx.as_mut())?;
                }
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

    /// Runs `tick` once per frame and redraws the canvas afterwards.
    ///
    /// A retained draw callback only runs when the canvas is dirty, so feeding
    /// it new data from a plain ticker paints once and then looks frozen: the
    /// data changed but nothing said so. This ties the two together.
    ///
    /// Return `false` from `tick` to stop; the ticker unregisters itself.
    /// Otherwise it stops when the canvas is dropped.
    ///
    /// `tick` runs on the UI thread but is declared `Send` because the frame
    /// scheduler holds it, so it cannot capture the canvas itself. It does not
    /// need to: the redraw is arranged here.
    ///
    /// ```rust,no_run
    /// # use aurea::render::{Canvas, RendererBackend};
    /// # fn main() -> aurea::AureaResult<()> {
    /// let canvas = Canvas::new(400, 300, RendererBackend::Cpu)?;
    /// canvas.set_draw_callback(|ctx| { let _ = ctx; Ok(()) })?;
    /// canvas.on_frame(|info| {
    ///     let _ = info.delta;
    ///     true
    /// });
    /// # Ok(())
    /// # }
    /// ```
    pub fn on_frame<F>(&self, mut tick: F) -> TickerId
    where
        F: FnMut(FrameInfo) -> bool + Send + 'static,
    {
        let handle = handle_key(self.handle);
        FrameScheduler::register_ticker(move |info| {
            if !tick(info) {
                return false;
            }
            // A canvas that has gone away makes this a no-op, so a ticker
            // outliving its canvas cannot touch a dead handle.
            request_canvas_redraw(handle);
            true
        })
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
    ///
    /// The closure is invoked every frame with [`aurea_runtime::FrameInfo`]
    /// (time, delta, frame counter). Return `true` to continue or `false` to
    /// stop — the ticker unregisters itself automatically on `false`.
    ///
    /// Returns a [`aurea_runtime::TickerId`] that can be passed to
    /// [`FrameScheduler::unregister_ticker`] for early cancellation.
    ///
    /// # Example
    /// ```rust,ignore
    /// use std::time::Duration;
    /// use aurea::FrameInfo;
    /// use aurea_animation::{Animation, EaseMode};
    ///
    /// let mut anim = Animation::new(Duration::from_secs(1)).ease(EaseMode::OutCubic);
    /// let id = canvas.animate(move |info: FrameInfo| {
    ///     match anim.tick(info.delta) {
    ///         Some(t) => { /* update app state with t */ true }
    ///         None    => false,
    ///     }
    /// });
    /// ```
    pub fn animate<F>(&self, ticker: F) -> TickerId
    where
        F: FnMut(FrameInfo) -> bool + Send + 'static,
    {
        let state = self.state.clone();
        let handle_usize = self.handle as usize;
        let mut user_ticker = ticker;

        FrameScheduler::register_ticker(move |info| {
            let keep = user_ticker(info);
            // Mark the canvas dirty so the scheduler's needs_redraw gate is
            // satisfied on every animation frame, including the final one.
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
    /// `x` and `y` are in logical (point) coordinates.
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
    /// `x` and `y` are in logical (point) coordinates.
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

impl Element for Canvas {
    fn released_to_parent(&self) {
        // Clones share one cleanup, so this is recorded once for the canvas
        // however many handles exist.
        self._cleanup.owns_native.store(false, Ordering::Release);
    }

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
