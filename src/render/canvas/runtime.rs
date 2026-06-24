use crate::ffi::*;
use crate::render::canvas::{ensure_canvas_renderer, Canvas, CanvasState};
use crate::render::{Surface, SurfaceInfo};
use crate::view::FrameScheduler;
use crate::{AureaError, AureaResult};
use aurea_render::{CURRENT_BUFFER, Renderer, RendererBackend};
use std::os::raw::c_void;
use std::sync::{Arc, Mutex};
use crate::sync::lock;
use aurea_render::Rect;
use std::ptr::{copy_nonoverlapping, null_mut};

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
    let dst =
        unsafe { ng_platform_canvas_acquire_buffer(handle, w, h, &mut stride_px, &mut buffer_index) };

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
    state: &Arc<Mutex<CanvasState>>,
    renderer: &Arc<Mutex<Option<Box<dyn Renderer>>>>,
    handle: *mut c_void,
    _backend: RendererBackend,
) -> AureaResult<()> {
    let (damage_rect, draw_callback, bg_color) = {
        let mut st = lock(state);
        let damage = st.damage.take();
        let cb = st.draw_callback.clone();
        let bg = st.background_color;
        (damage, cb, bg)
    };

    let current_damage = {
        let mut r = lock(renderer);
        if let Some(ref mut r) = *r {
            r.set_damage(damage_rect);
            let mut ctx = r.begin_frame()?;
            ctx.clear(bg_color)?;
            if let Some(ref cb) = draw_callback {
                cb(ctx.as_mut())?;
            }
            r.end_frame()?;
            r.last_frame_damage()
        } else {
            None
        }
    };

    #[cfg(feature = "zengpu")]
    let publishes_cpu_buffer = _backend != RendererBackend::ZenGpu;
    #[cfg(not(feature = "zengpu"))]
    let publishes_cpu_buffer = true;
    if publishes_cpu_buffer
        && let Some((ptr, size, w, h)) = CURRENT_BUFFER.with(|buf| *buf.borrow())
        && !ptr.is_null()
        && size > 0
    {
        let refresh = {
            let mut st = lock(state);
            let prev = st.prev_frame_damage;
            st.prev_frame_damage = current_damage;
            union_opt_rects(current_damage, prev)
        };
        unsafe {
            publish_cpu_buffer(handle, ptr, size, w, h, refresh);
        }
    }

    Ok(())
}

impl Canvas {
    pub(super) fn register_with_scheduler(
        &self,
        state: Arc<Mutex<CanvasState>>,
        renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
        backend: RendererBackend,
    ) {
        let handle_usize = self.handle as usize;

        let callback: Arc<dyn Fn() -> AureaResult<()> + Send + Sync> = Arc::new(move || {
            let handle = handle_usize as *mut c_void;

            let mut width: u32 = 0;
            let mut height: u32 = 0;
            unsafe {
                ng_platform_canvas_get_size(handle, &mut width, &mut height);
            }
            let new_scale = unsafe {
                let window = ng_platform_canvas_get_window(handle);
                if !window.is_null() {
                    ng_platform_get_scale_factor(window)
                } else {
                    lock(&state).scale_factor
                }
            };

            let (size_changed, scale_changed, cur_w, cur_h) = {
                let mut st = lock(&state);
                let size_changed =
                    width > 0 && height > 0 && (width != st.width || height != st.height);
                let scale_changed = (new_scale - st.scale_factor).abs() > f32::EPSILON;
                if size_changed {
                    st.width = width;
                    st.height = height;
                }
                if scale_changed {
                    st.scale_factor = new_scale;
                }
                (size_changed, scale_changed, st.width, st.height)
            };

            if !ensure_canvas_renderer(handle, &state, &renderer, backend)? {
                lock(&state).needs_redraw = true;
                return Ok(());
            }

            if size_changed || scale_changed {
                CURRENT_BUFFER.with(|buf| {
                    *buf.borrow_mut() = None;
                });

                let mut r = lock(&renderer);
                if let Some(ref mut r) = *r {
                    if scale_changed {
                        r.init(
                            Surface::OpenGL {
                                context: null_mut(),
                            },
                            SurfaceInfo {
                                width: cur_w,
                                height: cur_h,
                                scale_factor: new_scale,
                            },
                        )?;
                    }
                    if size_changed {
                        r.resize(cur_w, cur_h)?;
                    }
                }
                drop(r);
                lock(&state).needs_redraw = true;
            }

            let should_redraw = {
                let mut st = lock(&state);
                if !st.needs_redraw {
                    return Ok(());
                }
                st.needs_redraw = false;
                true
            };

            if should_redraw {
                render_frame(&state, &renderer, handle, backend)?;
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
        render_frame(&self.state, &self.renderer, self.handle, self.backend)
    }

    pub(super) fn check_and_resize(&mut self) -> AureaResult<()> {
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        unsafe {
            ng_platform_canvas_get_size(self.handle, &mut width, &mut height);
        }
        let new_scale = unsafe {
            let window = ng_platform_canvas_get_window(self.handle);
            if !window.is_null() {
                ng_platform_get_scale_factor(window)
            } else {
                lock(&self.state).scale_factor
            }
        };

        if width == 0 || height == 0 {
            return Ok(());
        }

        let (size_changed, scale_changed, cur_w, cur_h) = {
            let mut st = lock(&self.state);
            let sc = width != st.width || height != st.height;
            let sca = (new_scale - st.scale_factor).abs() > f32::EPSILON;
            if sc {
                st.width = width;
                st.height = height;
            }
            if sca {
                st.scale_factor = new_scale;
            }
            (sc, sca, st.width, st.height)
        };

        if !size_changed && !scale_changed {
            let _ = ensure_canvas_renderer(self.handle, &self.state, &self.renderer, self.backend)?;
            return Ok(());
        }

        CURRENT_BUFFER.with(|buf| {
            *buf.borrow_mut() = None;
        });

        if !ensure_canvas_renderer(self.handle, &self.state, &self.renderer, self.backend)? {
            return Ok(());
        }

        let mut r = lock(&self.renderer);
        if let Some(ref mut r) = *r {
            if scale_changed {
                r.init(
                    Surface::OpenGL {
                        context: null_mut(),
                    },
                    SurfaceInfo {
                        width: cur_w,
                        height: cur_h,
                        scale_factor: new_scale,
                    },
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
        if let Some((ptr, size, w, h)) = CURRENT_BUFFER.with(|buf| *buf.borrow())
            && !ptr.is_null()
            && size > 0
        {
            unsafe {
                publish_cpu_buffer(self.handle, ptr, size, w, h, None);
            }
        }
    }
}
