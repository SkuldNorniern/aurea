use super::*;
use crate::render::{Surface, SurfaceInfo};

/// Unified render pipeline: damage → begin_frame → clear → draw callback → end_frame → platform update.
///
/// Locking order: state (brief snapshot) → release → renderer (full render) → release.
/// The draw callback runs with NO lock held, so it can safely read canvas state
/// (size(), background_color(), etc.) without deadlocking.
fn render_frame(
    state: &Arc<Mutex<CanvasState>>,
    renderer: &Arc<Mutex<Option<Box<dyn Renderer>>>>,
    handle: *mut c_void,
    _backend: RendererBackend,
) -> AureaResult<()> {
    // 1. Snapshot what we need, then release the state lock.
    let (damage_rect, draw_callback, bg_color) = {
        let mut st = crate::sync::lock(state);
        let damage = st.damage.take();
        let cb = st.draw_callback.clone(); // Arc clone — O(1), no deep copy
        let bg = st.background_color;
        (damage, cb, bg)
    };

    // 2. Render under renderer lock only.
    {
        let mut r = crate::sync::lock(renderer);
        if let Some(ref mut r) = *r {
            r.set_damage(damage_rect);
            let mut ctx = r.begin_frame()?;
            ctx.clear(bg_color)?;
            if let Some(ref cb) = draw_callback {
                cb(ctx.as_mut())?; // state lock NOT held here
            }
            r.end_frame()?;
        }
    }

    // 3. Push buffer to platform (CURRENT_BUFFER is thread-local; no lock needed).
    #[cfg(feature = "zengpu")]
    let publishes_cpu_buffer = _backend != RendererBackend::ZenGpu;
    #[cfg(not(feature = "zengpu"))]
    let publishes_cpu_buffer = true;
    if publishes_cpu_buffer {
        if let Some((ptr, size, w, h)) = CURRENT_BUFFER.with(|buf| *buf.borrow()) {
            if !ptr.is_null() && size > 0 {
                unsafe {
                    ng_platform_canvas_update_buffer(handle, ptr, size as u32, w, h);
                }
            }
        }
    }

    Ok(())
}

impl Canvas {
    /// Called from Canvas::new — registers this canvas with the frame scheduler.
    pub(super) fn register_with_scheduler(
        &self,
        state: Arc<Mutex<CanvasState>>,
        renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
        backend: RendererBackend,
    ) {
        let handle_usize = self.handle as usize;

        let callback: Arc<dyn Fn() -> AureaResult<()> + Send + Sync> = Arc::new(move || {
            let handle = handle_usize as *mut c_void;

            // Query platform for current size and scale factor.
            let mut width: u32 = 0;
            let mut height: u32 = 0;
            unsafe { ng_platform_canvas_get_size(handle, &mut width, &mut height); }
            let new_scale = unsafe {
                let window = ng_platform_canvas_get_window(handle);
                if !window.is_null() {
                    ng_platform_get_scale_factor(window)
                } else {
                    crate::sync::lock(&state).scale_factor
                }
            };

            // Detect size/scale changes and update state.
            let (size_changed, scale_changed, cur_w, cur_h) = {
                let mut st = crate::sync::lock(&state);
                let size_changed =
                    width > 0 && height > 0 && (width != st.width || height != st.height);
                let scale_changed = (new_scale - st.scale_factor).abs() > f32::EPSILON;
                if size_changed { st.width = width; st.height = height; }
                if scale_changed { st.scale_factor = new_scale; }
                (size_changed, scale_changed, st.width, st.height)
            };

            if !ensure_canvas_renderer(handle, &state, &renderer, backend)? {
                crate::sync::lock(&state).needs_redraw = true;
                return Ok(());
            }

            if size_changed || scale_changed {
                // Null the platform pointer before any realloc so the stale raw
                // pointer never escapes to the platform layer.
                CURRENT_BUFFER.with(|buf| { *buf.borrow_mut() = None; });

                let mut r = crate::sync::lock(&renderer);
                if let Some(ref mut r) = *r {
                    if scale_changed {
                        // Surface::Cpu once step 15 is done; placeholder until then.
                        r.init(
                            Surface::OpenGL { context: std::ptr::null_mut() },
                            SurfaceInfo { width: cur_w, height: cur_h, scale_factor: new_scale },
                        )?;
                    }
                    if size_changed {
                        r.resize(cur_w, cur_h)?;
                    }
                }
                drop(r);
                crate::sync::lock(&state).needs_redraw = true;
            }

            // Gate on needs_redraw to avoid redundant redraws.
            let should_redraw = {
                let mut st = crate::sync::lock(&state);
                if !st.needs_redraw { return Ok(()); }
                st.needs_redraw = false;
                true
            };

            if should_redraw {
                render_frame(&state, &renderer, handle, backend)?;
                // After rendering, trigger a platform repaint so the new buffer
                // is displayed (e.g. WM_PAINT on Windows, setNeedsDisplay on macOS).
                // render_frame only pushes pixels to the canvas buffer — the platform
                // still needs a paint event to blit that buffer onto the screen.
                unsafe { ng_platform_canvas_invalidate(handle); }
            }

            Ok(())
        });

        FrameScheduler::register_canvas(self.handle, callback);
    }

    /// Internal: perform a full redraw now (called from redraw_if_needed and draw()).
    pub(super) fn perform_redraw(&mut self) -> AureaResult<()> {
        self.check_and_resize()?;
        if !ensure_canvas_renderer(self.handle, &self.state, &self.renderer, self.backend)? {
            return Err(AureaError::ElementOperationFailed);
        }
        render_frame(&self.state, &self.renderer, self.handle, self.backend)
    }

    pub(super) fn check_and_resize(&mut self) -> AureaResult<()> {
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        unsafe { ng_platform_canvas_get_size(self.handle, &mut width, &mut height); }
        let new_scale = unsafe {
            let window = ng_platform_canvas_get_window(self.handle);
            if !window.is_null() {
                ng_platform_get_scale_factor(window)
            } else {
                crate::sync::lock(&self.state).scale_factor
            }
        };

        if width == 0 || height == 0 {
            return Ok(());
        }

        let (size_changed, scale_changed, cur_w, cur_h) = {
            let mut st = crate::sync::lock(&self.state);
            let sc = width != st.width || height != st.height;
            let sca = (new_scale - st.scale_factor).abs() > f32::EPSILON;
            if sc { st.width = width; st.height = height; }
            if sca { st.scale_factor = new_scale; }
            (sc, sca, st.width, st.height)
        };

        if !size_changed && !scale_changed {
            let _ = ensure_canvas_renderer(self.handle, &self.state, &self.renderer, self.backend)?;
            return Ok(());
        }

        CURRENT_BUFFER.with(|buf| { *buf.borrow_mut() = None; });

        if !ensure_canvas_renderer(self.handle, &self.state, &self.renderer, self.backend)? {
            return Ok(());
        }

        let mut r = crate::sync::lock(&self.renderer);
        if let Some(ref mut r) = *r {
            if scale_changed {
                r.init(
                    Surface::OpenGL { context: std::ptr::null_mut() },
                    SurfaceInfo { width: cur_w, height: cur_h, scale_factor: new_scale },
                )?;
            }
            if size_changed {
                r.resize(cur_w, cur_h)?;
            }
        }

        Ok(())
    }

    pub(super) fn update_platform_view(&self) {
        #[cfg(feature = "zengpu")]
        if self.backend == RendererBackend::ZenGpu {
            return;
        }
        if let Some((ptr, size, w, h)) = CURRENT_BUFFER.with(|buf| *buf.borrow()) {
            if !ptr.is_null() && size > 0 {
                unsafe {
                    ng_platform_canvas_update_buffer(self.handle, ptr, size as u32, w, h);
                }
            }
        }
    }
}
