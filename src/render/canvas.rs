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

/// Drawing callback — Arc so it can be cheaply cloned out of the state lock
/// before the renderer lock is acquired, preventing deadlock when the callback
/// reads canvas properties (size, background_color, etc.).
pub type DrawCallback =
    Arc<dyn Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync>;

/// All per-frame mutable canvas properties in one lock.
/// Renderer lives in a separate Arc<Mutex<>> so render_frame can release this
/// lock before invoking the draw callback.
pub(crate) struct CanvasState {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub damage: DamageRegion,
    pub background_color: Color,
    pub draw_callback: Option<DrawCallback>,
    pub needs_redraw: bool,
}

/// Unregisters the canvas from the scheduler and tears down the renderer when
/// the *last* `Canvas` clone is dropped.
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
    #[allow(dead_code)]
    platform: Platform,
    #[allow(dead_code)]
    capabilities: CapabilityChecker,
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
        let st = crate::sync::lock(&self.state);
        (st.width, st.height)
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

        let renderer = match backend {
            RendererBackend::Cpu => {
                let mut renderer: Box<dyn Renderer> = Box::new(CpuRasterizer::new(width, height));
                renderer.init(
                    Surface::OpenGL {
                        context: std::ptr::null_mut(),
                    },
                    SurfaceInfo {
                        width,
                        height,
                        scale_factor: 1.0,
                    },
                )?;
                Some(renderer)
            }
            RendererBackend::Gpu => {
                let mut renderer: Box<dyn Renderer> = Box::new(GpuRasterizer::new(width, height));
                renderer.init(
                    Surface::OpenGL {
                        context: std::ptr::null_mut(),
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
        let renderer_arc = Arc::new(Mutex::new(renderer));
        let interaction_registry = Arc::new(InteractionRegistry::new());

        let canvas = Self {
            handle,
            state: state.clone(),
            renderer: renderer_arc.clone(),
            backend,
            interaction_registry,
            platform,
            capabilities,
            _cleanup: Arc::new(CanvasCleanup {
                handle: handle as usize,
                renderer: renderer_arc.clone(),
            }),
        };

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
            let mut st = crate::sync::lock(&self.state);
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
        if crate::sync::lock(&self.renderer).is_none() {
            return Err(AureaError::ElementOperationFailed);
        }

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
    pub fn animate<F>(&self, ticker: F) -> aurea_runtime::TickerId
    where
        F: FnMut(aurea_runtime::FrameInfo) -> bool + Send + 'static,
    {
        let state = self.state.clone();
        let handle_usize = self.handle as usize;
        let mut user_ticker = ticker;

        aurea_runtime::FrameScheduler::register_ticker(move |info| {
            let keep = user_ticker(info);
            // Mark the canvas dirty so the scheduler's needs_redraw gate is
            // satisfied on every animation frame, including the final one.
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
    /// `x` and `y` are in logical (point) coordinates.
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
    /// `x` and `y` are in logical (point) coordinates.
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

#[cfg(feature = "zengpu")]
pub(super) fn ensure_canvas_renderer(
    handle: *mut c_void,
    state: &Arc<Mutex<CanvasState>>,
    renderer: &Arc<Mutex<Option<Box<dyn Renderer>>>>,
    backend: RendererBackend,
) -> AureaResult<bool> {
    if crate::sync::lock(renderer).is_some() {
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
        let st = crate::sync::lock(state);
        (st.width.max(1), st.height.max(1), st.scale_factor.max(1.0))
    };
    let gpu = aurea_render::ZenGpuRenderer::new(&handles, width, height, scale_factor)?;
    *crate::sync::lock(renderer) = Some(Box::new(gpu));
    Ok(true)
}

#[cfg(not(feature = "zengpu"))]
pub(super) fn ensure_canvas_renderer(
    _handle: *mut c_void,
    _state: &Arc<Mutex<CanvasState>>,
    renderer: &Arc<Mutex<Option<Box<dyn Renderer>>>>,
    _backend: RendererBackend,
) -> AureaResult<bool> {
    Ok(crate::sync::lock(renderer).is_some())
}

#[cfg(all(feature = "zengpu", target_os = "windows"))]
fn zengpu_canvas_handles(handle: *mut c_void) -> AureaResult<zengpu_hal::WindowHandles> {
    use raw_window_handle::{
        RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
    };
    use std::num::NonZeroIsize;

    let hwnd = NonZeroIsize::new(handle as isize).ok_or(AureaError::ElementOperationFailed)?;
    Ok(zengpu_hal::WindowHandles::from_raw(
        RawWindowHandle::Win32(Win32WindowHandle::new(hwnd)),
        RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
    ))
}

#[cfg(all(feature = "zengpu", target_os = "macos"))]
fn zengpu_canvas_handles(handle: *mut c_void) -> AureaResult<zengpu_hal::WindowHandles> {
    use raw_window_handle::{
        AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
    };
    use std::ptr::NonNull;

    let view = unsafe { ng_platform_canvas_get_native_handle(handle) };
    let view = NonNull::new(view).ok_or(AureaError::ElementOperationFailed)?;
    Ok(zengpu_hal::WindowHandles::from_raw(
        RawWindowHandle::AppKit(AppKitWindowHandle::new(view)),
        RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
    ))
}

#[cfg(all(feature = "zengpu", target_os = "linux"))]
fn zengpu_canvas_handles(handle: *mut c_void) -> AureaResult<zengpu_hal::WindowHandles> {
    use raw_window_handle::{
        RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
        XcbDisplayHandle, XcbWindowHandle,
    };
    use std::{num::NonZeroU32, ptr::NonNull};

    let mut xcb_window = 0;
    let mut xcb_connection = std::ptr::null_mut();
    if unsafe {
        crate::ffi::ng_platform_canvas_get_xcb_handle(
            handle,
            &mut xcb_window,
            &mut xcb_connection,
        )
    } != 0
    {
        let window = NonZeroU32::new(xcb_window).ok_or(AureaError::ElementOperationFailed)?;
        let connection =
            NonNull::new(xcb_connection).ok_or(AureaError::ElementOperationFailed)?;
        return Ok(zengpu_hal::WindowHandles::from_raw(
            RawWindowHandle::Xcb(XcbWindowHandle::new(window)),
            RawDisplayHandle::Xcb(XcbDisplayHandle::new(Some(connection), 0)),
        ));
    }

    let mut surface = std::ptr::null_mut();
    let mut display = std::ptr::null_mut();
    if unsafe {
        crate::ffi::ng_platform_canvas_get_wayland_handle(handle, &mut surface, &mut display)
    } != 0
    {
        let surface = NonNull::new(surface).ok_or(AureaError::ElementOperationFailed)?;
        let display = NonNull::new(display).ok_or(AureaError::ElementOperationFailed)?;
        return Ok(zengpu_hal::WindowHandles::from_raw(
            RawWindowHandle::Wayland(WaylandWindowHandle::new(surface)),
            RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display)),
        ));
    }

    Err(AureaError::ElementOperationFailed)
}

#[cfg(all(
    feature = "zengpu",
    not(any(target_os = "windows", target_os = "macos", target_os = "linux"))
))]
fn zengpu_canvas_handles(_handle: *mut c_void) -> AureaResult<zengpu_hal::WindowHandles> {
    Err(AureaError::ElementOperationFailed)
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

        assert!(crate::sync::lock(&canvas.renderer).is_none());
        assert!(
            !ensure_canvas_renderer(
                canvas.handle,
                &canvas.state,
                &canvas.renderer,
                canvas.backend,
            )
            .unwrap()
        );
        assert!(crate::sync::lock(&canvas.renderer).is_none());
    }
}

