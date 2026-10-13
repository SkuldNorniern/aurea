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
use std::ptr::null_mut;
use std::rc::Rc;
#[cfg(feature = "zengpu")]
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "zengpu")]
use aurea_render::ZenGpuContext;
#[cfg(feature = "wgpu")]
use wgpu::{Instance, Surface as WgpuSurface, SurfaceTarget};

mod backend;
mod driver;
mod lifecycle;
mod state;
mod weak;

pub(crate) use lifecycle::notify_canvas_destroyed;

use backend::ensure_canvas_renderer;
use lifecycle::{CanvasCleanup, next_canvas_id, register_canvas_state};
pub use lifecycle::{request_canvas_redraw, request_canvas_redraw_at};
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
    fn ensure_live(&self) -> AureaResult<()> {
        if self._cleanup.live.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(AureaError::ElementOperationFailed)
        }
    }
    /// Get the native window handle for this canvas.
    pub fn native_handle(&self) -> *mut c_void {
        if self.ensure_live().is_err() {
            return null_mut();
        }
        unsafe { ng_platform_canvas_get_native_handle(self.handle) }
    }

    /// Get the parent window handle for this canvas.
    pub fn window_handle(&self) -> *mut c_void {
        if self.ensure_live().is_err() {
            return null_mut();
        }
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
        self.ensure_live()?;
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
            #[cfg(feature = "zengpu")]
            gpu_context: None,
            #[cfg(feature = "zengpu")]
            gpu_lost: None,
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
                live: AtomicBool::new(true),
                warned_about_mixed_drawing: AtomicBool::new(false),
            }),
        };

        guard.disarm();
        lifecycle::register_native_canvas(&canvas._cleanup);
        register_canvas_state(id, handle_key(handle));
        canvas.register_with_scheduler(state, renderer_arc, backend);
        Ok(canvas)
    }

    /// A ZenGPU canvas drawing on `context`'s device, so textures the app makes there can be drawn
    /// into it with [`DrawingContext::draw_texture`].
    #[cfg(feature = "zengpu")]
    pub fn with_gpu_context(
        width: u32,
        height: u32,
        context: Arc<ZenGpuContext>,
    ) -> AureaResult<Self> {
        let canvas = Self::new(width, height, RendererBackend::ZenGpu)?;
        canvas.state.borrow_mut().gpu_context = Some(context);
        Ok(canvas)
    }

    /// The device a ZenGPU canvas draws with: the one it was given, or the one it made once it was
    /// first shown.
    #[cfg(feature = "zengpu")]
    pub fn gpu_context(&self) -> Option<Arc<ZenGpuContext>> {
        self.state.borrow().gpu_context.clone()
    }

    /// Draws on `context` from the next frame: the renderer on the old one goes first, since a
    /// window takes one surface at a time.
    #[cfg(feature = "zengpu")]
    pub fn set_gpu_context(&self, context: Arc<ZenGpuContext>) {
        *self.renderer.borrow_mut() = None;
        let mut st = self.state.borrow_mut();
        st.gpu_context = Some(context);
        st.needs_redraw = true;
        st.damage.add_all();
        drop(st);
        FrameScheduler::schedule_canvas(self.handle);
    }

    /// Asks `handler` for a context to draw on once this canvas's device is lost, after a driver
    /// reset or a GPU being removed.
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use aurea::render::{Canvas, ZenGpuContext};
    /// # fn f(canvas: &Canvas) {
    /// canvas.on_gpu_lost(|lost| {
    ///     let fresh = Arc::new(lost.reopen().ok()?);
    ///     // upload the app's textures to `fresh` again here
    ///     Some(fresh)
    /// });
    /// # }
    /// ```
    #[cfg(feature = "zengpu")]
    pub fn on_gpu_lost(
        &self,
        handler: impl FnMut(&Arc<ZenGpuContext>) -> Option<Arc<ZenGpuContext>> + 'static,
    ) {
        self.state.borrow_mut().gpu_lost = Some(Box::new(handler));
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
        let guard = self.renderer.try_borrow().ok()?;
        let frame = guard.as_ref()?.frame_output()?;
        Some(f(frame.words(), frame.width, frame.height))
    }

    /// Set the drawing callback (retained-mode style).
    pub fn set_draw_callback<F>(&self, callback: F) -> AureaResult<()>
    where
        F: FnMut(&mut dyn DrawingContext) -> AureaResult<()> + 'static,
    {
        self.ensure_live()?;
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
        if self
            .renderer
            .try_borrow_mut()
            .map_err(|_| AureaError::RenderingFailed)?
            .is_none()
        {
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
                    let mut ctx = renderer
                        .begin_frame()
                        .inspect_err(|_| driver::retry_failed_frame(self.handle, &self.state))?;
                    ctx.clear(bg_color)?;
                    draw_fn(ctx.as_mut())
                        .inspect_err(|_| driver::retry_failed_frame(self.handle, &self.state))?;
                }
                if self.ensure_live().is_err() {
                    *r = None;
                    return Err(AureaError::ElementOperationFailed);
                }
                let outcome = renderer
                    .end_frame()
                    .inspect_err(|_| driver::retry_failed_frame(self.handle, &self.state))?;
                if outcome != FrameOutcome::Presented {
                    drop(r);
                    driver::follow_outcome(outcome, self.handle, &self.state, &self.renderer);
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
        if self.ensure_live().is_err() {
            return;
        }
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
        if self.ensure_live().is_err() {
            return;
        }
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
        if self.ensure_live().is_err() {
            return;
        }
        if self._cleanup.owns_native.swap(false, Ordering::AcqRel) {
            unsafe { ng_platform_element_released_to_parent(self.handle) };
        }
    }

    fn handle(&self) -> *mut c_void {
        if self.ensure_live().is_ok() {
            self.handle
        } else {
            null_mut()
        }
    }

    unsafe fn invalidate_platform(&self, rect: Option<super::Rect>) {
        if self.ensure_live().is_err() {
            return;
        }
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
    use std::cell::Cell;

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

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn a_lost_context_is_replaced() {
        let lost = Arc::new(ZenGpuContext::new().unwrap());
        let canvas = Canvas::with_gpu_context(64, 64, Arc::clone(&lost)).unwrap();
        lost.device().simulate_loss();
        driver::replace_lost_context(&canvas.state);
        let reopened = canvas.gpu_context().unwrap();
        assert!(!Arc::ptr_eq(&reopened, &lost) && !reopened.is_lost());
        assert_eq!(reopened.adapter_uuid(), lost.adapter_uuid(), "same GPU");

        let given = Arc::new(ZenGpuContext::new().unwrap());
        let handed = Arc::clone(&given);
        let asked = Rc::new(Cell::new(0));
        let count = Rc::clone(&asked);
        canvas.on_gpu_lost(move |_| {
            count.set(count.get() + 1);
            Some(Arc::clone(&handed))
        });
        reopened.device().simulate_loss();
        driver::replace_lost_context(&canvas.state);
        assert!(Arc::ptr_eq(&canvas.gpu_context().unwrap(), &given));
        assert_eq!(asked.get(), 1);
        driver::replace_lost_context(&canvas.state);
        assert_eq!(asked.get(), 1, "a live context is left alone");
    }

    #[test]
    fn a_given_context_is_the_one_drawn_with() {
        let context = Arc::new(ZenGpuContext::new().unwrap());
        let canvas = Canvas::with_gpu_context(64, 64, Arc::clone(&context)).unwrap();
        assert!(Arc::ptr_eq(&canvas.gpu_context().unwrap(), &context));
        assert!(
            Canvas::new(8, 8, RendererBackend::ZenGpu)
                .unwrap()
                .gpu_context()
                .is_none()
        );
    }
}

#[cfg(test)]
mod repaint_tests {
    //! Repaint behaviour of a live canvas inside a real window.

    use crate::elements::{Orientation, Stack};
    use crate::render::{Canvas, Color, DrawingContext, Paint, PaintStyle, Rect, RendererBackend};
    use crate::{AureaResult, Container, Window};
    use aurea_runtime::FrameScheduler;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::sleep;
    use std::time::Duration;

    const BG: Color = Color {
        r: 240,
        g: 240,
        b: 240,
        a: 255,
    };

    fn paint_a_square(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        let fill = Paint::new()
            .color(Color::rgb(100, 150, 200))
            .style(PaintStyle::Fill);
        ctx.draw_rect(Rect::new(10.0, 10.0, 100.0, 100.0), &fill)
    }

    fn drawn_pixels(canvas: &Canvas) -> usize {
        let bg = (u32::from(BG.a) << 24)
            | (u32::from(BG.r) << 16)
            | (u32::from(BG.g) << 8)
            | u32::from(BG.b);
        canvas
            .with_frame_pixels(|pixels, _, _| pixels.iter().filter(|&&p| p != bg).count())
            .unwrap_or(0)
    }

    fn settle(window: &Window) -> AureaResult<()> {
        for _ in 0..20 {
            window.poll_events();
            window.process_frames()?;
            sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    fn window_with_canvas(title: &str) -> AureaResult<(Window, Canvas)> {
        let mut window = Window::new(title, 400, 300)?;
        let canvas = Canvas::new(400, 300, RendererBackend::Cpu)?;
        canvas.set_background_color(BG);
        let mut layout = Stack::new(Orientation::Vertical)?;
        layout.add(canvas.clone())?;
        window.set_content(layout)?;
        window.show();
        settle(&window)?;
        Ok((window, canvas))
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn retained_draw_survives_the_layout_resize() -> AureaResult<()> {
        let mut window = Window::new("retained", 400, 300)?;
        let canvas = Canvas::new(400, 300, RendererBackend::Cpu)?;
        canvas.set_background_color(BG);
        canvas.set_draw_callback(paint_a_square)?;

        let mut layout = Stack::new(Orientation::Vertical)?;
        layout.add(canvas.clone())?;
        window.set_content(layout)?;
        window.show();

        for _ in 0..3 {
            window.poll_events();
            window.process_frames()?;
        }

        assert!(
            drawn_pixels(&canvas) > 1000,
            "retained canvas went blank after the window resized it"
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn invalidate_keeps_immediate_mode_pixels() -> AureaResult<()> {
        let (window, mut canvas) = window_with_canvas("immediate")?;

        canvas.draw(paint_a_square)?;
        let before = drawn_pixels(&canvas);
        assert!(before > 1000, "immediate draw produced nothing");

        canvas.invalidate_all();
        for _ in 0..3 {
            window.poll_events();
            window.process_frames()?;
        }

        assert_eq!(
            drawn_pixels(&canvas),
            before,
            "invalidate_all wiped an immediate-mode canvas"
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn on_frame_keeps_the_draw_callback_running() -> AureaResult<()> {
        let (window, canvas) = window_with_canvas("on_frame")?;

        let draws = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&draws);
        canvas.set_draw_callback(move |ctx| {
            counter.fetch_add(1, Ordering::Relaxed);
            let n = counter.load(Ordering::Relaxed) % 40;
            let fill = Paint::new()
                .color(Color::rgb(100, 150, 200))
                .style(PaintStyle::Fill);
            ctx.draw_rect(Rect::new(n as f32, 10.0, 20.0, 20.0), &fill)
        })?;

        let ticks = Arc::new(AtomicUsize::new(0));
        let tick_counter = Arc::clone(&ticks);
        let ticker = canvas.on_frame(move |_| {
            tick_counter.fetch_add(1, Ordering::Relaxed);
            true
        });

        for _ in 0..4 {
            window.poll_events();
            window.process_frames()?;
        }
        FrameScheduler::unregister_ticker(ticker);

        assert!(
            ticks.load(Ordering::Relaxed) >= 3,
            "the ticker should run each frame"
        );
        assert!(
            draws.load(Ordering::Relaxed) >= 3,
            "the canvas redrew {} time(s); a ticker alone leaves it at 1",
            draws.load(Ordering::Relaxed)
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn on_frame_stops_when_it_returns_false() -> AureaResult<()> {
        let (window, canvas) = window_with_canvas("on_frame stop")?;
        canvas.set_draw_callback(|_| Ok(()))?;

        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&ticks);
        canvas.on_frame(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
            false
        });

        for _ in 0..4 {
            window.poll_events();
            window.process_frames()?;
        }

        assert_eq!(
            ticks.load(Ordering::Relaxed),
            1,
            "it should run once and stop"
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates native elements; run with --ignored"]
    fn dropping_a_canvas_destroys_the_native_element() -> AureaResult<()> {
        use crate::elements::Element;

        let _window = Window::new("canvas ownership", 200, 100)?;

        let canvas = Canvas::new(100, 100, RendererBackend::Cpu)?;
        let handle = canvas.handle();
        assert!(!handle.is_null());

        let clone = canvas.clone();
        drop(canvas);
        drop(clone);
        Ok(())
    }

    #[test]
    #[ignore = "creates native widgets; run with --ignored"]
    fn canvas_clones_free_once() -> AureaResult<()> {
        let _window = Window::new("canvas clones", 300, 200)?;

        for _ in 0..100 {
            let canvas = Canvas::new(64, 64, RendererBackend::Cpu)?;
            let a = canvas.clone();
            let b = canvas.clone();
            drop(canvas);
            drop(a);
            drop(b);
        }
        Ok(())
    }

    #[test]
    #[ignore = "creates native elements; run with --ignored"]
    fn a_draw_callback_can_capture_its_own_canvas() -> AureaResult<()> {
        use std::cell::Cell;
        use std::rc::Rc;

        let mut window = Window::new("capture", 300, 200)?;
        let canvas = Canvas::new(300, 200, RendererBackend::Cpu)?;
        canvas.set_background_color(BG);

        let frames = Rc::new(Cell::new(0usize));
        let counted = Rc::clone(&frames);
        let for_callback = canvas.clone();

        canvas.set_draw_callback(move |ctx| {
            counted.set(counted.get() + 1);
            let (w, _h) = for_callback.size();
            paint_a_square(ctx)?;
            ctx.draw_rect(
                Rect::new(0.0, 0.0, w as f32 / 4.0, 4.0),
                &Paint::new().color(Color::rgb(200, 120, 60)),
            )
        })?;

        let mut layout = Stack::new(Orientation::Vertical)?;
        layout.add(canvas.clone())?;
        window.set_content(layout)?;
        window.show();

        for _ in 0..3 {
            window.poll_events();
            window.process_frames()?;
        }

        assert!(frames.get() > 0, "the callback never ran");
        assert!(drawn_pixels(&canvas) > 1000, "and it drew nothing");
        Ok(())
    }
}
