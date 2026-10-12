use crate::elements::Element;
use crate::ffi::*;
#[cfg(feature = "wgpu")]
use crate::platform::handles::{NativeWindowHandle, native_handle_from_canvas_ptr};
use crate::registry::handle_key;
use crate::{AureaError, AureaResult};
use aurea_render::{
    ClickCallback, Color, CpuRasterizer, DrawingContext, FrameOutcome, HoverCallback,
    InteractionRegistry, InteractiveId, Point, Renderer, RendererBackend, Surface, SurfaceInfo,
};
use aurea_runtime::{DamageRegion, FrameScheduler};
use aurea_runtime::{FrameInfo, TickerId};
use std::cell::RefCell;
use std::os::raw::c_void;
use std::rc::Rc;
use std::slice::from_raw_parts;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "wgpu")]
use wgpu::{Instance, Surface as WgpuSurface, SurfaceTarget};

mod backend;
mod driver;
mod lifecycle;
mod state;
mod weak;

use backend::ensure_canvas_renderer;
pub use lifecycle::request_canvas_redraw;
use lifecycle::{CanvasCleanup, next_canvas_id, register_canvas_state};
pub use state::CanvasId;
use state::CanvasState;
pub use weak::WeakCanvas;

/// What a canvas draws, held by the UI thread that runs it.
pub type DrawCallback = Rc<RefCell<dyn FnMut(&mut dyn DrawingContext) -> AureaResult<()>>>;

/// A drawable canvas element backed by a renderer.
#[derive(Clone)]
pub struct Canvas {
    pub(crate) handle: *mut c_void,
    pub(crate) state: Rc<RefCell<CanvasState>>,
    pub(crate) renderer: Rc<RefCell<Option<Box<dyn Renderer>>>>,
    pub(crate) backend: RendererBackend,
    interaction_registry: Rc<InteractionRegistry>,
    #[cfg(feature = "wgpu")]
    surface_handle: Rc<NativeWindowHandle>,
    _cleanup: Rc<CanvasCleanup>,
}

struct CanvasBuildGuard {
    handle: *mut c_void,
    armed: bool,
}

impl CanvasBuildGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CanvasBuildGuard {
    fn drop(&mut self) {
        if self.armed {
            unsafe { ng_platform_destroy_element(self.handle) };
        }
    }
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
        let st = self.state.borrow_mut();
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

        let mut guard = CanvasBuildGuard {
            handle,
            armed: true,
        };
        let id = next_canvas_id();

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

        let state = Rc::new(RefCell::new(CanvasState {
            width,
            height,
            scale_factor,
            damage: DamageRegion::new(16),
            background_color: Color::rgb(255, 255, 255),
            draw_callback: None,
            needs_redraw: false,
            prev_frame_damage: None,
        }));
        let renderer_arc = Rc::new(RefCell::new(renderer));
        let interaction_registry = Rc::new(InteractionRegistry::new());

        #[cfg(feature = "wgpu")]
        let surface_handle = Rc::new(
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
            _cleanup: Rc::new(CanvasCleanup {
                handle: handle_key(handle),
                id,
                renderer: renderer_arc.clone(),
                owns_native: AtomicBool::new(true),
                warned_about_mixed_drawing: AtomicBool::new(false),
            }),
        };

        guard.disarm();
        register_canvas_state(id, handle_key(handle));
        canvas.register_with_scheduler(state, renderer_arc, backend);
        Ok(canvas)
    }

    /// This canvas's identity, for as long as the process runs.
    pub fn id(&self) -> CanvasId {
        self._cleanup.id
    }

    fn warn_if_also_retained(&self) {
        if self.state.borrow_mut().draw_callback.is_none() {
            return;
        }
        if self
            ._cleanup
            .warned_about_mixed_drawing
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        log::warn!(
            "Canvas::draw called on a canvas that also has a draw callback.              Both paint the same buffer and the scheduler re-runs the              callback on its own, so the two will take turns and the region              will flicker. Use one or the other for a given canvas."
        );
    }

    /// Runs `f` with the pixels this canvas last drew, as `0xAARRGGBB` words.
    pub fn with_frame_pixels<R>(&self, f: impl FnOnce(&[u32], u32, u32) -> R) -> Option<R> {
        let guard = self.renderer.borrow_mut();
        let frame = guard.as_ref()?.frame_output()?;
        // SAFETY: the renderer is held for the length of the call, so its
        let bytes = unsafe { from_raw_parts(frame.pixels, frame.size) };
        let pixels: Vec<u32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&c| u32::from_le_bytes(c))
            .collect();
        Some(f(&pixels, frame.width, frame.height))
    }

    /// Set the drawing callback (retained-mode style).
    pub fn set_draw_callback<F>(&self, callback: F) -> AureaResult<()>
    where
        F: FnMut(&mut dyn DrawingContext) -> AureaResult<()> + 'static,
    {
        {
            let mut st = self.state.borrow_mut();
            st.draw_callback = Some(Rc::new(RefCell::new(callback)));
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
        if self.renderer.borrow_mut().is_none() {
            return Err(AureaError::ElementOperationFailed);
        }
        self.warn_if_also_retained();

        let (damage_rect, bg_color) = {
            let mut st = self.state.borrow_mut();
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
            let mut r = self.renderer.borrow_mut();
            if let Some(ref mut renderer) = *r {
                renderer.set_damage(damage_rect);
                {
                    let mut ctx = renderer.begin_frame()?;
                    ctx.clear(bg_color)?;
                    draw_fn(ctx.as_mut())?;
                }
                if renderer.end_frame()? != FrameOutcome::Presented {
                    return Ok(());
                }
            }
        }

        self.update_platform_view();
        Ok(())
    }

    /// Set background color.
    pub fn set_background_color(&self, color: Color) {
        let changed = {
            let mut st = self.state.borrow_mut();
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
        self.state.borrow_mut().background_color
    }

    /// Runs `tick` once per frame and redraws the canvas afterwards.
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
        let id = self.id();
        FrameScheduler::register_ticker(move |info| {
            let keep = tick(info);
            request_canvas_redraw(id);
            keep
        })
    }

    /// Add damage to the canvas (called when content changes).
    pub fn add_damage(&self, rect: super::Rect) {
        self.state.borrow_mut().damage.add(rect);
    }

    /// Mark the entire canvas as damaged and schedule a redraw.
    pub fn invalidate_all(&self) {
        {
            let mut st = self.state.borrow_mut();
            st.damage.add_all();
            st.needs_redraw = true;
        }
        FrameScheduler::schedule_canvas(self.handle);
    }

    /// Check if canvas needs redraw and perform it.
    pub fn redraw_if_needed(&mut self) -> AureaResult<()> {
        let needs = {
            let mut st = self.state.borrow_mut();
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
            let mut st = self.state.borrow_mut();
            st.damage.add(rect);
            st.needs_redraw = true;
        }
        FrameScheduler::schedule_canvas(self.handle);
    }

    pub fn width(&self) -> u32 {
        self.size().0
    }
    pub fn height(&self) -> u32 {
        self.size().1
    }

    /// Old name for [`Self::on_frame`].
    #[deprecated(note = "use `Canvas::on_frame`, which does the same")]
    pub fn animate<F>(&self, ticker: F) -> TickerId
    where
        F: FnMut(FrameInfo) -> bool + Send + 'static,
    {
        self.on_frame(ticker)
    }

    pub fn scale_factor(&self) -> f32 {
        self.state.borrow_mut().scale_factor
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
        let r = self.renderer.borrow_mut();
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
        let r = self.renderer.borrow_mut();
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
        if self._cleanup.owns_native.swap(false, Ordering::AcqRel) {
            unsafe { ng_platform_element_released_to_parent(self.handle) };
        }
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

        assert!(canvas.renderer.borrow().is_none());
        assert!(
            !ensure_canvas_renderer(
                canvas.handle,
                &canvas.state,
                &canvas.renderer,
                canvas.backend,
            )
            .unwrap()
        );
        assert!(canvas.renderer.borrow().is_none());
    }
}
