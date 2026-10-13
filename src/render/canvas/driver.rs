use super::lifecycle::{canvas_is_live, take_full_redraw_request};
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
        let mut slot = renderer.borrow_mut();
        if let Some(ref mut r) = *slot {
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
            if !canvas_is_live(handle) {
                *slot = None;
                return Err(AureaError::ElementOperationFailed);
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

pub(super) fn follow_outcome(
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
    configure_surface(
        state,
        renderer,
        SurfaceInfo {
            width,
            height,
            scale_factor: scale,
        },
    )?;
    Ok(true)
}

fn configure_surface(
    state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
    info: SurfaceInfo,
) -> AureaResult<()> {
    let changed = {
        let st = state.borrow();
        info.width != st.width
            || info.height != st.height
            || (info.scale_factor - st.scale_factor).abs() > f32::EPSILON
    };
    if changed {
        if let Some(r) = renderer
            .try_borrow_mut()
            .map_err(|_| AureaError::RenderingFailed)?
            .as_mut()
        {
            r.configure(info.clone())?;
        }
        let mut st = state.borrow_mut();
        st.width = info.width;
        st.height = info.height;
        st.scale_factor = info.scale_factor;
        st.needs_redraw = true;
        st.damage.add_all();
    }
    Ok(())
}

pub(super) fn retry_failed_frame(handle: *mut c_void, state: &Rc<RefCell<CanvasState>>) {
    if canvas_is_live(handle) {
        let mut st = state.borrow_mut();
        st.needs_redraw = true;
        st.damage.add_all();
        drop(st);
        FrameScheduler::schedule_canvas(handle);
    }
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
        let (ptr, size, w, h) = (frame.pixels(), frame.size(), frame.width, frame.height);
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
            if !canvas_is_live(handle) {
                return Ok(());
            }

            if take_full_redraw_request(id) {
                let mut st = state.borrow_mut();
                st.damage.add_all();
                st.needs_redraw = true;
            }

            if !ensure_canvas_renderer(handle, &state, &renderer, backend)
                .inspect_err(|_| retry_failed_frame(handle, &state))?
            {
                state.borrow_mut().needs_redraw = true;
                return Ok(());
            }
            if !sync_surface(handle, &state, &renderer)
                .inspect_err(|_| retry_failed_frame(handle, &state))?
            {
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
                let outcome = render_frame(&state, &renderer, handle, backend)
                    .inspect_err(|_| retry_failed_frame(handle, &state))?;
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
        let outcome = render_frame(&self.state, &self.renderer, self.handle, self.backend)
            .inspect_err(|_| retry_failed_frame(self.handle, &self.state))?;
        follow_outcome(outcome, self.handle, &self.state, &self.renderer);
        Ok(())
    }

    pub(super) fn check_and_resize(&mut self) -> AureaResult<()> {
        self.ensure_live()?;
        if ensure_canvas_renderer(self.handle, &self.state, &self.renderer, self.backend)? {
            sync_surface(self.handle, &self.state, &self.renderer)
                .inspect_err(|_| retry_failed_frame(self.handle, &self.state))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use aurea_render::{Color, CpuRasterizer, DrawingContext, FrameOutput, Surface};
    use aurea_runtime::DamageRegion;
    use std::cell::Cell;

    struct FailOnce {
        raster: CpuRasterizer,
        attempts: Rc<Cell<u32>>,
    }

    impl Renderer for FailOnce {
        fn init(&mut self, surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
            self.raster.init(surface, info)
        }
        fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
            self.raster.resize(width, height)
        }
        fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()> {
            self.attempts.set(self.attempts.get() + 1);
            if self.attempts.get() == 1 {
                return Err(AureaError::RenderingFailed);
            }
            self.raster.configure(info)
        }
        fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext + '_>> {
            self.raster.begin_frame()
        }
        fn end_frame(&mut self) -> AureaResult<FrameOutcome> {
            self.raster.end_frame()
        }
        fn cleanup(&mut self) {
            self.raster.cleanup();
        }
        fn frame_output(&self) -> Option<FrameOutput<'_>> {
            self.raster.frame_output()
        }
    }

    #[test]
    fn a_failed_configuration_is_retried_before_committing_the_extent() {
        let state = Rc::new(RefCell::new(CanvasState {
            width: 16,
            height: 16,
            scale_factor: 1.0,
            damage: DamageRegion::new(16),
            background_color: Color::rgb(0, 0, 0),
            draw_callback: None,
            needs_redraw: false,
            prev_frame_damage: None,
            #[cfg(feature = "zengpu")]
            gpu_context: None,
        }));
        let attempts = Rc::new(Cell::new(0));
        let renderer: Rc<RefCell<Option<Box<dyn Renderer>>>> =
            Rc::new(RefCell::new(Some(Box::new(FailOnce {
                raster: CpuRasterizer::new(16, 16),
                attempts: Rc::clone(&attempts),
            }))));
        let info = SurfaceInfo {
            width: 32,
            height: 24,
            scale_factor: 2.0,
        };
        assert!(configure_surface(&state, &renderer, info.clone()).is_err());
        assert_eq!(
            (
                state.borrow().width,
                state.borrow().height,
                state.borrow().scale_factor
            ),
            (16, 16, 1.0)
        );
        configure_surface(&state, &renderer, info.clone()).expect("retry");
        configure_surface(&state, &renderer, info).expect("unchanged extent");
        assert_eq!(attempts.get(), 2);
        assert_eq!(
            (
                state.borrow().width,
                state.borrow().height,
                state.borrow().scale_factor
            ),
            (32, 24, 2.0)
        );
        assert!(state.borrow().needs_redraw);
    }
}
