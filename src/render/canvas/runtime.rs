use super::*;
use crate::render::{Surface, SurfaceInfo};

fn render_frame(
    state: &Arc<Mutex<CanvasState>>,
    renderer: &Arc<Mutex<Option<Box<dyn Renderer>>>>,
    handle: *mut c_void,
) -> AureaResult<()> {
    let (damage_rect, draw_callback, bg_color) = {
        let mut st = crate::sync::lock(state);
        let damage = st.damage.take();
        let cb = st.draw_callback.clone();
        let bg = st.background_color;
        (damage, cb, bg)
    };

    {
        let mut r = crate::sync::lock(renderer);
        if let Some(ref mut r) = *r {
            r.set_damage(damage_rect);
            let mut ctx = r.begin_frame()?;
            ctx.clear(bg_color)?;
            if let Some(ref cb) = draw_callback {
                cb(ctx.as_mut())?;
            }
            r.end_frame()?;
        }
    }

    if let Some((ptr, size, w, h)) = CURRENT_BUFFER.with(|buf| *buf.borrow()) {
        if !ptr.is_null() && size > 0 {
            unsafe {
                ng_platform_canvas_update_buffer(handle, ptr, size as u32, w, h);
            }
        }
    }

    Ok(())
}

impl Canvas {
    pub(super) fn register_with_scheduler(
        &self,
        state: Arc<Mutex<CanvasState>>,
        renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
    ) {
        let handle_usize = self.handle as usize;

        let callback: Arc<dyn Fn() -> AureaResult<()> + Send + Sync> = Arc::new(move || {
            let handle = handle_usize as *mut c_void;

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

            let (size_changed, scale_changed, cur_w, cur_h) = {
                let mut st = crate::sync::lock(&state);
                let size_changed =
                    width > 0 && height > 0 && (width != st.width || height != st.height);
                let scale_changed = (new_scale - st.scale_factor).abs() > f32::EPSILON;
                if size_changed { st.width = width; st.height = height; }
                if scale_changed { st.scale_factor = new_scale; }
                (size_changed, scale_changed, st.width, st.height)
            };

            if size_changed || scale_changed {
                CURRENT_BUFFER.with(|buf| { *buf.borrow_mut() = None; });

                let mut r = crate::sync::lock(&renderer);
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
                drop(r);
                crate::sync::lock(&state).needs_redraw = true;
            }

            let should_redraw = {
                let mut st = crate::sync::lock(&state);
                if !st.needs_redraw { return Ok(()); }
                st.needs_redraw = false;
                true
            };

            if should_redraw {
                render_frame(&state, &renderer, handle)?;
                unsafe { ng_platform_canvas_invalidate(handle); }
            }

            Ok(())
        });

        FrameScheduler::register_canvas(self.handle, callback);
    }

    pub(super) fn perform_redraw(&mut self) -> AureaResult<()> {
        self.check_and_resize()?;
        render_frame(&self.state, &self.renderer, self.handle)
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
            return Ok(());
        }

        CURRENT_BUFFER.with(|buf| { *buf.borrow_mut() = None; });

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
        if let Some((ptr, size, w, h)) = CURRENT_BUFFER.with(|buf| *buf.borrow()) {
            if !ptr.is_null() && size > 0 {
                unsafe {
                    ng_platform_canvas_update_buffer(self.handle, ptr, size as u32, w, h);
                }
            }
        }
    }
}
