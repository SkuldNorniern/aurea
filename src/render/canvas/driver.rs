//! Drawing a canvas's frame and showing it, for its scheduler entry and its own calls alike.

use super::backend::ensure_canvas_renderer;
use super::lifecycle::{canvas_id_is_live, take_frame_request, take_full_redraw_request};
use super::state::{CanvasId, CanvasState, MaybeRenderer, RendererSlot};
use crate::ffi::*;
use crate::platform::presentation::cpu::publish_cpu_buffer;
use crate::render::SurfaceInfo;
use crate::render::canvas::Canvas;
use crate::{AureaError, AureaResult};
use aurea_render::{DrawingContext, FrameOutcome, Rect, RendererBackend};
use aurea_runtime::FrameScheduler;
use std::cell::RefCell;
use std::mem;
use std::os::raw::c_void;
use std::ptr::null;
use std::rc::Rc;
#[cfg(feature = "zengpu")]
use std::sync::Arc;

/// What a frame needs of a canvas, borrowed from the canvas or its scheduler entry.
pub(super) struct Driver<'a> {
    pub id: CanvasId,
    pub handle: *mut c_void,
    pub state: &'a RefCell<CanvasState>,
    pub renderer: &'a RendererSlot,
    pub backend: RendererBackend,
}

impl Driver<'_> {
    /// Asks the scheduler for another full frame after a failure, unless the canvas is gone.
    fn retry(&self) {
        if canvas_id_is_live(self.id) {
            self.state.borrow_mut().redraw_all();
            FrameScheduler::schedule_canvas(self.handle);
        }
    }

    fn retrying<T>(&self, result: AureaResult<T>) -> AureaResult<T> {
        result.inspect_err(|_| self.retry())
    }

    /// Whether the canvas has a renderer, making a GPU one once its view is attached.
    pub fn ensure_renderer(&self) -> AureaResult<bool> {
        ensure_canvas_renderer(self.handle, self.state, self.renderer, self.backend)
    }

    /// Follows the platform's size and scale; false while the view has no area.
    pub fn sync_surface(&self) -> AureaResult<bool> {
        let (mut width, mut height) = (0u32, 0u32);
        let (mut pixel_width, mut pixel_height) = (0u32, 0u32);
        unsafe {
            ng_platform_canvas_get_size(self.handle, &mut width, &mut height);
            ng_platform_canvas_get_pixel_size(self.handle, &mut pixel_width, &mut pixel_height);
        }
        if width == 0 || height == 0 {
            return Ok(false);
        }
        let window = unsafe { ng_platform_canvas_get_window(self.handle) };
        let scale_factor = if window.is_null() {
            self.state.borrow().scale_factor
        } else {
            unsafe { ng_platform_get_scale_factor(window) }
        };
        let info = SurfaceInfo {
            width,
            height,
            scale_factor,
            pixels: (pixel_width > 0 && pixel_height > 0).then_some((pixel_width, pixel_height)),
        };
        if self.retrying(configure_surface(self.state, self.renderer, info))? {
            // SAFETY: a null buffer tells the canvas to drop its pointer into the old frame
            // buffer, which configuring may have freed.
            unsafe { ng_platform_canvas_update_buffer(self.handle, null(), 0, 0, 0) };
        }
        Ok(true)
    }

    /// Draws a frame over `damage` with `draw`, shows it, and follows what the renderer said.
    pub fn paint(
        &self,
        damage: Option<Rect>,
        draw: impl FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    ) -> AureaResult<()> {
        // a draw from inside a draw finds the renderer taken; that is no reason to draw again
        let mut slot = self
            .renderer
            .try_borrow_mut()
            .map_err(|_| AureaError::RenderingFailed)?;
        let result = self.paint_frame(&mut slot, damage, draw);
        drop(slot);
        let (outcome, drawn) = self.retrying(result)?;
        match outcome {
            FrameOutcome::Presented => self.publish(drawn),
            outcome => self.follow(outcome),
        }
        Ok(())
    }

    fn paint_frame(
        &self,
        slot: &mut MaybeRenderer,
        damage: Option<Rect>,
        draw: impl FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    ) -> AureaResult<(FrameOutcome, Option<Rect>)> {
        let background = self.state.borrow().background_color;
        let Some(renderer) = slot.as_mut() else {
            return Ok((FrameOutcome::Retry, None));
        };
        renderer.set_damage(damage);
        {
            let mut ctx = renderer.begin_frame()?;
            ctx.clear(background)?;
            draw(ctx.as_mut())?;
        }
        // the draw may have dropped the canvas
        if !canvas_id_is_live(self.id) {
            *slot = None;
            return Err(AureaError::ElementOperationFailed);
        }
        let outcome = renderer.end_frame()?;
        Ok((outcome, renderer.last_frame_damage()))
    }

    /// Draws a retained frame: the draw callback over the damage, or the last frame again when
    /// there is no callback.
    pub fn render(&self) -> AureaResult<()> {
        let (damage, callback) = {
            let mut st = self.state.borrow_mut();
            (st.damage.take(), st.draw_callback.clone())
        };
        let Some(callback) = callback else {
            let drawn = self
                .renderer
                .try_borrow()
                .is_ok_and(|r| r.as_ref().is_some_and(|r| r.frame_output().is_some()));
            if drawn {
                self.publish(None);
                return Ok(());
            }
            return self.paint(damage, |_| Ok(()));
        };
        self.paint(damage, |ctx| match callback.try_borrow_mut() {
            Ok(mut callback) => callback(ctx),
            Err(_) => Ok(()),
        })
    }

    /// Hands a CPU frame to the platform, refreshing what this frame and the one before drew;
    /// `None` refreshes all of it.
    fn publish(&self, drawn: Option<Rect>) {
        #[cfg(feature = "zengpu")]
        if self.backend == RendererBackend::ZenGpu {
            return;
        }
        let slot = self.renderer.borrow();
        let Some(frame) = slot.as_ref().and_then(|r| r.frame_output()) else {
            return;
        };
        let refresh = {
            let mut st = self.state.borrow_mut();
            let before = mem::replace(&mut st.prev_frame_damage, drawn);
            drawn.zip(before).map(|(a, b)| a.union(b))
        };
        unsafe {
            publish_cpu_buffer(
                self.handle,
                frame.pixels(),
                frame.size(),
                frame.width,
                frame.height,
                refresh,
            );
        }
    }

    /// Acts on a frame the renderer did not show.
    fn follow(&self, outcome: FrameOutcome) {
        match outcome {
            FrameOutcome::Presented | FrameOutcome::Unchanged => {}
            FrameOutcome::Suspended => self.state.borrow_mut().needs_redraw = true,
            FrameOutcome::Retry => {
                self.state.borrow_mut().needs_redraw = true;
                FrameScheduler::schedule_canvas(self.handle);
            }
            FrameOutcome::Recreate => {
                *self.renderer.borrow_mut() = None;
                #[cfg(feature = "zengpu")]
                replace_lost_context(self.state);
                self.state.borrow_mut().redraw_all();
                FrameScheduler::schedule_canvas(self.handle);
            }
        }
    }

    /// The scheduler's turn for this canvas: takes the frame asks, then draws when one is due.
    fn serve(&self, id: CanvasId) -> AureaResult<()> {
        if !canvas_id_is_live(self.id) {
            return Ok(());
        }
        let frame = take_frame_request(id);
        if take_full_redraw_request(id) {
            self.state.borrow_mut().redraw_all();
        } else if frame {
            self.state.borrow_mut().needs_redraw = true;
        }
        if !self.retrying(self.ensure_renderer())? || !self.sync_surface()? {
            self.state.borrow_mut().needs_redraw = true;
            return Ok(());
        }
        if !self.state.borrow_mut().take_redraw() {
            return Ok(());
        }
        self.render()
    }
}

#[cfg(feature = "zengpu")]
pub(super) fn replace_lost_context(state: &RefCell<CanvasState>) {
    let (lost, mut handler, default_context, no_recovery) = {
        let mut st = state.borrow_mut();
        let Some(lost) = st.gpu_context.clone().filter(|c| c.is_lost()) else {
            return;
        };
        (
            lost,
            st.gpu_lost.take(),
            st.gpu_context_default,
            st.gpu_no_recovery,
        )
    };
    let chosen = handler.as_mut().and_then(|handler| handler(&lost));
    let handler_chose = chosen.is_some();
    // the handler may have installed a context itself
    let replaced = state
        .borrow()
        .gpu_context
        .as_ref()
        .is_some_and(|current| !Arc::ptr_eq(current, &lost));
    let fresh = if replaced {
        None
    } else {
        chosen.or_else(|| {
            if no_recovery {
                None
            } else if default_context {
                super::backend::default_gpu_context(Some(lost.adapter_uuid())).ok()
            } else {
                lost.reopen().ok().map(Arc::new)
            }
        })
    };
    let mut st = state.borrow_mut();
    if st.gpu_lost.is_none() {
        st.gpu_lost = handler;
    }
    if !replaced {
        st.gpu_context = fresh;
        st.gpu_context_default = default_context && !handler_chose;
    }
}

/// Configures the renderer for `info` when it differs from the last; the state keeps the old
/// extent until that succeeds.
fn configure_surface(
    state: &RefCell<CanvasState>,
    renderer: &RendererSlot,
    info: SurfaceInfo,
) -> AureaResult<bool> {
    let changed = {
        let st = state.borrow();
        info.width != st.width
            || info.height != st.height
            || info.pixels != st.pixels
            || (info.scale_factor - st.scale_factor).abs() > f32::EPSILON
    };
    if !changed {
        return Ok(false);
    }
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
    st.pixels = info.pixels;
    st.redraw_all();
    Ok(true)
}

impl Canvas {
    pub(super) fn driver(&self) -> Driver<'_> {
        Driver {
            id: self.id(),
            handle: self.handle,
            state: &self.state,
            renderer: &self.renderer,
            backend: self.backend,
        }
    }

    pub(super) fn register_with_scheduler(&self) {
        let handle = self.handle as usize;
        let id = self.id();
        let state = Rc::clone(&self.state);
        let renderer = Rc::clone(&self.renderer);
        let backend = self.backend;
        let callback: Rc<dyn Fn() -> AureaResult<()>> = Rc::new(move || {
            Driver {
                id,
                handle: handle as *mut c_void,
                state: &state,
                renderer: &renderer,
                backend,
            }
            .serve(id)
        });
        FrameScheduler::register_canvas(self.handle, callback);
    }

    /// The driver with its renderer and surface up to date, for a draw outside the scheduler.
    pub(super) fn ready(&self) -> AureaResult<Driver<'_>> {
        self.ensure_live()?;
        let driver = self.driver();
        if !driver.ensure_renderer()? {
            return Err(AureaError::ElementOperationFailed);
        }
        driver.sync_surface()?;
        Ok(driver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurea_render::{CpuRasterizer, FrameOutput, Renderer, Surface};
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
        let state = RefCell::new(CanvasState::new(16, 16, 1.0));
        let attempts = Rc::new(Cell::new(0));
        let renderer: RendererSlot = Rc::new(RefCell::new(Some(Box::new(FailOnce {
            raster: CpuRasterizer::new(16, 16),
            attempts: Rc::clone(&attempts),
        }))));
        let info = SurfaceInfo {
            width: 32,
            height: 24,
            scale_factor: 2.0,
            pixels: None,
        };
        let extent = |state: &RefCell<CanvasState>| {
            let st = state.borrow();
            (st.width, st.height, st.scale_factor)
        };
        assert!(configure_surface(&state, &renderer, info.clone()).is_err());
        assert_eq!(extent(&state), (16, 16, 1.0));
        assert!(configure_surface(&state, &renderer, info.clone()).expect("retry"));
        assert!(!configure_surface(&state, &renderer, info).expect("unchanged extent"));
        assert_eq!(attempts.get(), 2);
        assert_eq!(extent(&state), (32, 24, 2.0));
        assert!(state.borrow().needs_redraw);
    }
}
