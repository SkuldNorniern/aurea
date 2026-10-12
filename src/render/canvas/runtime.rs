use crate::ffi::*;
use crate::render::SurfaceInfo;
use crate::render::canvas::state::take_full_redraw_request;
use crate::render::canvas::{Canvas, CanvasState, ensure_canvas_renderer};
use crate::{AureaError, AureaResult};
use aurea_render::Rect;
use aurea_render::{FrameOutcome, Renderer, RendererBackend};
use aurea_runtime::FrameScheduler;
use std::cell::RefCell;
use std::os::raw::c_void;
use std::ptr::copy_nonoverlapping;
use std::rc::Rc;

fn f32_to_usize_clamped(v: f32) -> usize {
    let v = v.clamp(0.0, 16_777_216.0);
    // SAFETY: v is clamped to a non-negative, exactly-representable range above.
    unsafe { v.to_int_unchecked() }
}

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

unsafe fn publish_cpu_buffer(
    handle: *mut c_void,
    ptr: *const u8,
    size: usize,
    w: u32,
    h: u32,
    refresh: Option<Rect>,
) {
    let mut stride_px: u32 = 0;
    let mut buffer_index: u32 = 0;
    let dst = unsafe {
        ng_platform_canvas_acquire_buffer(handle, w, h, &mut stride_px, &mut buffer_index)
    };

    if dst.is_null() {
        unsafe {
            ng_platform_canvas_update_buffer(
                handle,
                ptr,
                u32::try_from(size).expect("buffer size fits in u32"),
                w,
                h,
            );
            match refresh {
                Some(r) => {
                    ng_platform_canvas_invalidate_rect(handle, r.x, r.y, r.width, r.height);
                }
                None => {
                    ng_platform_canvas_invalidate(handle);
                }
            }
        }
        return;
    }

    let src = ptr as *const u32;
    let dst = dst as *mut u32;
    unsafe {
        match refresh {
            None => {
                if stride_px == w {
                    copy_nonoverlapping(src, dst, w as usize * h as usize);
                } else {
                    for y in 0..h as usize {
                        copy_nonoverlapping(
                            src.add(y * w as usize),
                            dst.add(y * stride_px as usize),
                            w as usize,
                        );
                    }
                }
            }
            Some(r) => {
                let y0 = f32_to_usize_clamped(r.y.floor().max(0.0));
                let y1 = f32_to_usize_clamped((r.y + r.height).ceil().min(h as f32));
                for y in y0..y1 {
                    copy_nonoverlapping(
                        src.add(y * w as usize),
                        dst.add(y * stride_px as usize),
                        w as usize,
                    );
                }
            }
        }
        ng_platform_canvas_present(handle);
    }
}

fn render_frame(
    state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
    handle: *mut c_void,
    _backend: RendererBackend,
) -> AureaResult<FrameOutcome> {
    let (damage_rect, draw_callback, bg_color) = {
        let mut st = state.borrow_mut();
        let damage = st.damage.take();
        let cb = st.draw_callback.clone();
        let bg = st.background_color;
        (damage, cb, bg)
    };

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
            {
                let mut ctx = r.begin_frame()?;
                ctx.clear(bg_color)?;
                if let Some(mut cb) = draw_callback
                    .as_ref()
                    .and_then(|cb| cb.try_borrow_mut().ok())
                {
                    cb(ctx.as_mut())?;
                }
            }
            let outcome = r.end_frame()?;
            (outcome, r.last_frame_damage())
        } else {
            (FrameOutcome::Retry, None)
        }
    };
    let (outcome, current_damage) = current_damage;

    if outcome == FrameOutcome::Presented {
        republish_current_buffer(state, renderer, handle, _backend, current_damage);
    }

    Ok(outcome)
}

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
        let (ptr, size, w, h) = (frame.pixels, frame.size, frame.width, frame.height);
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

            if take_full_redraw_request(id) {
                let mut st = state.borrow_mut();
                st.damage.add_all();
                st.needs_redraw = true;
            }

            if !ensure_canvas_renderer(handle, &state, &renderer, backend)? {
                state.borrow_mut().needs_redraw = true;
                return Ok(());
            }
            if !sync_surface(handle, &state, &renderer)? {
                state.borrow_mut().needs_redraw = true;
                return Ok(());
            }

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
        let guard = self.renderer.borrow_mut();
        if let Some(frame) = guard.as_ref().and_then(|r| r.frame_output()) {
            unsafe {
                publish_cpu_buffer(
                    self.handle,
                    frame.pixels,
                    frame.size,
                    frame.width,
                    frame.height,
                    None,
                );
            }
        }
    }
}
