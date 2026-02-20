use super::*;
use crate::render::{Surface, SurfaceInfo};

impl Canvas {
    fn handle_resize(
        &mut self,
        new_width: u32,
        new_height: u32,
        new_scale: f32,
    ) -> AureaResult<()> {
        let (size_changed, scale_changed) = {
            let mut metrics = crate::sync::lock(self.metrics.as_ref());
            let size_changed = new_width != metrics.width || new_height != metrics.height;
            let scale_changed = new_scale != metrics.scale_factor;
            if size_changed {
                metrics.width = new_width;
                metrics.height = new_height;
            }
            if scale_changed {
                metrics.scale_factor = new_scale;
            }
            (size_changed, scale_changed)
        };

        if !size_changed && !scale_changed {
            return Ok(());
        }

        if scale_changed {
            let mut renderer_guard = crate::sync::lock(self.renderer.as_ref());
            if let Some(ref mut renderer) = *renderer_guard {
                let surface = Surface::OpenGL {
                    context: std::ptr::null_mut(),
                };
                let surface_info = SurfaceInfo {
                    width: new_width,
                    height: new_height,
                    scale_factor: new_scale,
                };
                renderer.init(surface, surface_info)?;
            }
        }

        CURRENT_BUFFER.with(|buf| {
            *buf.borrow_mut() = None;
        });

        if size_changed {
            let mut renderer_guard = crate::sync::lock(self.renderer.as_ref());
            if let Some(ref mut renderer) = *renderer_guard {
                renderer.resize(new_width, new_height)?;
            }
        }

        Ok(())
    }

    /// Internal: Redraw if needed (called automatically when invalidated)
    /// This performs the actual drawing using the stored callback
    /// Note: This is now handled by the frame scheduler callback
    /// Kept for backward compatibility if called directly
    pub(super) fn perform_redraw(&mut self) -> AureaResult<()> {
        self.check_and_resize()?;

        let has_callback = {
            let cb = crate::sync::lock(self.draw_callback.as_ref());
            cb.is_some()
        };

        if !has_callback {
            return Ok(());
        }

        {
            let mut renderer_guard = crate::sync::lock(self.renderer.as_ref());
            if let Some(ref mut renderer) = *renderer_guard {
                let damage_rect = {
                    let mut damage = crate::sync::lock(self.damage.as_ref());
                    damage.take()
                };

                renderer.set_damage(damage_rect);

                let mut ctx = renderer.begin_frame()?;

                let bg_color = *crate::sync::lock(self.background_color.as_ref());
                ctx.clear(bg_color)?;

                {
                    let cb = crate::sync::lock(self.draw_callback.as_ref());
                    if let Some(ref callback) = *cb {
                        callback(ctx.as_mut())?;
                    }
                }

                renderer.end_frame()?;
            }
        }

        self.update_platform_view();

        Ok(())
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
                let metrics = crate::sync::lock(self.metrics.as_ref());
                metrics.scale_factor
            }
        };

        if width > 0 && height > 0 {
            self.handle_resize(width, height, new_scale)?;
        }

        Ok(())
    }

    pub(super) fn update_platform_view(&self) {
        if let Some((ptr, size, width, height)) = self.get_render_buffer()
            && !ptr.is_null()
            && size > 0
        {
            unsafe {
                ng_platform_canvas_update_buffer(self.handle, ptr, size as u32, width, height);
            }
        }
    }

    fn get_render_buffer(&self) -> Option<(*const u8, usize, u32, u32)> {
        use crate::render::CURRENT_BUFFER;
        CURRENT_BUFFER.with(|buf| *buf.borrow())
    }

    /// Register this canvas with the frame scheduler
    /// This enables automatic redraw when frames are scheduled
    pub(super) fn register_with_scheduler(
        &self,
        renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
        metrics: Arc<Mutex<CanvasMetrics>>,
        damage: Arc<Mutex<DamageRegion>>,
        background_color: Arc<Mutex<Color>>,
        draw_callback: Arc<Mutex<Option<DrawCallback>>>,
        needs_redraw: Arc<Mutex<bool>>,
    ) {
        let handle_usize = self.handle as usize;

        let callback: Arc<dyn Fn() -> AureaResult<()> + Send + Sync> = Arc::new(move || {
            let mut width: u32 = 0;
            let mut height: u32 = 0;
            unsafe {
                let handle_ptr = handle_usize as *mut c_void;
                ng_platform_canvas_get_size(handle_ptr, &mut width, &mut height);
            }

            let new_scale = unsafe {
                let handle_ptr = handle_usize as *mut c_void;
                let window = ng_platform_canvas_get_window(handle_ptr);
                if !window.is_null() {
                    ng_platform_get_scale_factor(window)
                } else {
                    let current = crate::sync::lock(metrics.as_ref());
                    current.scale_factor
                }
            };

            let (size_changed, scale_changed, metrics_width, metrics_height) = {
                let mut current = crate::sync::lock(metrics.as_ref());
                let size_changed =
                    width > 0 && height > 0 && (width != current.width || height != current.height);
                let scale_changed = new_scale != current.scale_factor;
                if size_changed {
                    current.width = width;
                    current.height = height;
                }
                if scale_changed {
                    current.scale_factor = new_scale;
                }
                (size_changed, scale_changed, current.width, current.height)
            };

            if size_changed || scale_changed {
                let mut renderer_guard = crate::sync::lock(renderer.as_ref());
                if let Some(ref mut r) = *renderer_guard {
                    if scale_changed {
                        let surface = Surface::OpenGL {
                            context: std::ptr::null_mut(),
                        };
                        let surface_info = SurfaceInfo {
                            width: metrics_width,
                            height: metrics_height,
                            scale_factor: new_scale,
                        };
                        r.init(surface, surface_info)?;
                    }
                    if size_changed {
                        r.resize(metrics_width, metrics_height)?;
                    }
                }

                CURRENT_BUFFER.with(|buf| {
                    *buf.borrow_mut() = None;
                });

                let mut flag = crate::sync::lock(needs_redraw.as_ref());
                *flag = true;
            }

            let should_redraw = {
                let mut flag = crate::sync::lock(needs_redraw.as_ref());
                if !*flag {
                    return Ok(());
                }
                *flag = false;
                true
            };

            if !should_redraw {
                return Ok(());
            }

            let damage_rect = {
                let mut d = crate::sync::lock(damage.as_ref());
                d.take()
            };

            let mut renderer_guard = crate::sync::lock(renderer.as_ref());
            if let Some(ref mut r) = *renderer_guard {
                r.set_damage(damage_rect);

                let mut ctx = r.begin_frame()?;

                let bg_color = {
                    let bg = crate::sync::lock(background_color.as_ref());
                    *bg
                };
                ctx.clear(bg_color)?;

                {
                    let cb = crate::sync::lock(draw_callback.as_ref());
                    if let Some(ref callback_fn) = *cb {
                        callback_fn(ctx.as_mut())?;
                    }
                }

                r.end_frame()?;

                use crate::render::CURRENT_BUFFER;
                if let Some((ptr, size, w, h)) = CURRENT_BUFFER.with(|buf| *buf.borrow()) {
                    if !ptr.is_null() && size > 0 {
                        unsafe {
                            let handle_ptr = handle_usize as *mut c_void;
                            ng_platform_canvas_update_buffer(handle_ptr, ptr, size as u32, w, h);
                        }
                    }
                }
            }

            Ok(())
        });

        FrameScheduler::register_canvas(self.handle, callback);
    }
}
