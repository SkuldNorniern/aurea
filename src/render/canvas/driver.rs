use super::lifecycle::take_full_redraw_request;
use crate::ffi::*;
use crate::platform::presentation::cpu::publish_cpu_buffer;
use crate::render::SurfaceInfo;
use crate::render::canvas::{Canvas, CanvasState, ensure_canvas_renderer};
use crate::{AureaError, AureaResult};
use aurea_render::Rect;
use aurea_render::{FrameOutcome, Renderer, RendererBackend};
use aurea_runtime::FrameScheduler;
use std::cell::RefCell;
use std::os::raw::c_void;
use std::rc::Rc;

/// Union of two optional physical-pixel damage rects.
/// `None` encodes "full frame" — any `None` operand dominates.
fn union_opt_rects(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (None, _) | (_, None) => None,
        (Some(a), Some(b)) => {
            let x0 = a.x.min(b.x);
            let y0 = a.y.min(b.y);
            let x1 = (a.x + a.width).max(b.x + b.width);
            let y1 = (a.y + a.height).max(b.y + b.height);
            Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
        }
    }
}

/// Unified render pipeline: damage → begin_frame → clear → draw callback → end_frame → platform update.
///
/// Locking order: state (brief snapshot) → release → renderer (full render) → release.
/// The draw callback runs with NO lock held, so it can safely read canvas state
/// (size(), background_color(), etc.) without deadlocking.
fn render_frame(
    state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
    handle: *mut c_void,
    _backend: RendererBackend,
) -> AureaResult<FrameOutcome> {
    // 1. Snapshot what we need, then release the state lock.
    let (damage_rect, draw_callback, bg_color) = {
        let mut st = state.borrow_mut();
        let damage = st.damage.take();
        let cb = st.draw_callback.clone(); // Arc clone — O(1), no deep copy
        let bg = st.background_color;
        (damage, cb, bg)
    };

    // 2. Render under renderer lock only; grab last_frame_damage before releasing.
    //
    // With no retained draw callback there is nothing to re-render, and
    // clearing anyway would throw away whatever `Canvas::draw` put in the
    // buffer — which is how an immediate-mode canvas went blank on the first
    // repaint. Republish what is already there instead.
    //
    // Unless the buffer is gone: a resize drops it, and presenting nothing
    // leaves the platform with a stale pointer. Then an empty frame is the
    // only frame that can be produced.
    // This canvas's own renderer, not whichever one drew last on this thread.
    let has_buffer = renderer
        .borrow_mut()
        .as_ref()
        .is_some_and(|r| r.frame_output().is_some());
    if draw_callback.is_none() && has_buffer {
        republish_current_buffer(state, renderer, handle, _backend, None);
        return Ok(FrameOutcome::Presented);
    }

    let current_damage = {
        let mut r = renderer.borrow_mut();
        if let Some(ref mut r) = *r {
            r.set_damage(damage_rect);
            // The context borrows the renderer, so it has to go out of scope
            // before the frame can be ended.
            {
                let mut ctx = r.begin_frame()?;
                ctx.clear(bg_color)?;
                // A callback that draws this canvas again from inside itself
                // finds it busy; that frame is skipped, not deadlocked.
                if let Some(mut cb) = draw_callback
                    .as_ref()
                    .and_then(|cb| cb.try_borrow_mut().ok())
                {
                    cb(ctx.as_mut())?; // state lock NOT held here
                }
            }
            let outcome = r.end_frame()?;
            (outcome, r.last_frame_damage())
        } else {
            (FrameOutcome::Retry, None)
        }
    };
    let (outcome, current_damage) = current_damage;

    // 3. Push this canvas's pixels to the platform, when there are new ones.
    if outcome == FrameOutcome::Presented {
        republish_current_buffer(state, renderer, handle, _backend, current_damage);
    }

    Ok(outcome)
}

/// Acts on what a frame came to: a frame the target could not take is owed
/// and asked for again, and a renderer whose target is gone is dropped so
/// the next frame makes a new one.
fn follow_outcome(
    outcome: FrameOutcome,
    handle: *mut c_void,
    state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
) {
    match outcome {
        FrameOutcome::Presented | FrameOutcome::Unchanged => {}
        FrameOutcome::Suspended => state.borrow_mut().needs_redraw = true,
        FrameOutcome::Retry => {
            state.borrow_mut().needs_redraw = true;
            FrameScheduler::schedule_canvas(handle);
        }
        FrameOutcome::Recreate => {
            *renderer.borrow_mut() = None;
            let mut st = state.borrow_mut();
            st.needs_redraw = true;
            st.damage.add_all();
            drop(st);
            FrameScheduler::schedule_canvas(handle);
        }
    }
}

/// Reads the canvas's logical size and scale from the platform and, when
/// either changed, configures the renderer with both at once. Returns
/// `false` while the canvas has no size.
fn sync_surface(
    handle: *mut c_void,
    state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
) -> AureaResult<bool> {
    let (mut width, mut height) = (0u32, 0u32);
    unsafe {
        ng_platform_canvas_get_size(handle, &mut width, &mut height);
    }
    let window = unsafe { ng_platform_canvas_get_window(handle) };
    let scale = if window.is_null() {
        state.borrow_mut().scale_factor
    } else {
        unsafe { ng_platform_get_scale_factor(window) }
    };
    if width == 0 || height == 0 {
        return Ok(false);
    }
    let changed = {
        let mut st = state.borrow_mut();
        let changed = width != st.width
            || height != st.height
            || (scale - st.scale_factor).abs() > f32::EPSILON;
        if changed {
            st.width = width;
            st.height = height;
            st.scale_factor = scale;
            st.needs_redraw = true;
            st.damage.add_all();
        }
        changed
    };
    if changed && let Some(r) = renderer.borrow_mut().as_mut() {
        r.configure(SurfaceInfo {
            width,
            height,
            scale_factor: scale,
        })?;
    }
    Ok(true)
}

/// Hands the renderer's current pixels to the platform.
fn republish_current_buffer(
    state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
    handle: *mut c_void,
    _backend: RendererBackend,
    current_damage: Option<Rect>,
) {
    #[cfg(feature = "zengpu")]
    let publishes_cpu_buffer = _backend != RendererBackend::ZenGpu;
    #[cfg(not(feature = "zengpu"))]
    let publishes_cpu_buffer = true;
    let guard = renderer.borrow_mut();
    let output = guard.as_ref().and_then(|r| r.frame_output());
    if publishes_cpu_buffer && let Some(frame) = output {
        let (ptr, size, w, h) = (frame.pixels(), frame.size(), frame.width, frame.height);
        // The IOSurface double-buffer is 2 frames stale on the back surface, so
        // we must refresh any pixel that changed in either this frame OR the
        // previous frame. Read and update prev_frame_damage under a brief lock.
        let refresh = {
            let mut st = state.borrow_mut();
            let prev = st.prev_frame_damage;
            st.prev_frame_damage = current_damage;
            union_opt_rects(current_damage, prev)
        };
        unsafe {
            publish_cpu_buffer(handle, ptr, size, w, h, refresh);
        }
    }
}

impl Canvas {
    /// Called from Canvas::new — registers this canvas with the frame scheduler.
    pub(super) fn register_with_scheduler(
        &self,
        state: Rc<RefCell<CanvasState>>,
        renderer: Rc<RefCell<Option<Box<dyn Renderer>>>>,
        backend: RendererBackend,
    ) {
        let handle_usize = self.handle as usize;
        let id = self.id();

        let callback: Rc<dyn Fn() -> AureaResult<()>> = Rc::new(move || {
            let handle = handle_usize as *mut c_void;

            // Somebody off the UI thread asked for a full redraw. Marking the
            // canvas dirty is this thread's job: the state holds the draw
            // callback, and the asking thread has no business reaching it.
            if take_full_redraw_request(id) {
                let mut st = state.borrow_mut();
                st.damage.add_all();
                st.needs_redraw = true;
            }

            if !ensure_canvas_renderer(handle, &state, &renderer, backend)? {
                state.borrow_mut().needs_redraw = true;
                return Ok(());
            }
            // A canvas with no size, a minimized window's, draws nothing
            // until it has one again; the frame stays owed.
            if !sync_surface(handle, &state, &renderer)? {
                state.borrow_mut().needs_redraw = true;
                return Ok(());
            }

            // Gate on needs_redraw to avoid redundant redraws.
            let should_redraw = {
                let mut st = state.borrow_mut();
                if !st.needs_redraw {
                    return Ok(());
                }
                st.needs_redraw = false;
                true
            };

            if should_redraw {
                let outcome = render_frame(&state, &renderer, handle, backend)?;
                follow_outcome(outcome, handle, &state, &renderer);
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
        let outcome = render_frame(&self.state, &self.renderer, self.handle, self.backend)?;
        follow_outcome(outcome, self.handle, &self.state, &self.renderer);
        Ok(())
    }

    pub(super) fn check_and_resize(&mut self) -> AureaResult<()> {
        if ensure_canvas_renderer(self.handle, &self.state, &self.renderer, self.backend)? {
            sync_surface(self.handle, &self.state, &self.renderer)?;
        }
        Ok(())
    }

    pub(super) fn update_platform_view(&self) {
        #[cfg(feature = "zengpu")]
        if self.backend == RendererBackend::ZenGpu {
            return;
        }
        // This canvas's own renderer, so a second canvas on the same thread
        // cannot end up publishing its pixels here.
        let guard = self.renderer.borrow_mut();
        if let Some(frame) = guard.as_ref().and_then(|r| r.frame_output()) {
            unsafe {
                publish_cpu_buffer(
                    self.handle,
                    frame.pixels(),
                    frame.size(),
                    frame.width,
                    frame.height,
                    None,
                );
            }
        }
    }
}
