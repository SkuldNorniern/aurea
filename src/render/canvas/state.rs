use crate::ffi::*;
use crate::sync::lock;
use aurea_runtime::{DamageRegion, FrameScheduler};
#[cfg(feature = "zengpu")]
use crate::AureaError;
use crate::AureaResult;
use aurea_render::{Color, Rect, Renderer, RendererBackend};
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::{Arc, LazyLock, Mutex};

use super::DrawCallback;

pub(crate) struct CanvasState {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub damage: DamageRegion,
    pub background_color: Color,
    pub draw_callback: Option<DrawCallback>,
    pub needs_redraw: bool,
    /// Physical-pixel damage rect from the previous rendered frame.
    pub prev_frame_damage: Option<Rect>,
}

static CANVAS_STATES: LazyLock<Mutex<HashMap<usize, Arc<Mutex<CanvasState>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn register_canvas_state(handle: usize, state: Arc<Mutex<CanvasState>>) {
    lock(&CANVAS_STATES).insert(handle, state);
}

fn unregister_canvas_state(handle: usize) {
    lock(&CANVAS_STATES).remove(&handle);
}

/// Request a full redraw of a canvas identified by its raw handle.
pub fn request_canvas_redraw(handle: usize) {
    let state = lock(&CANVAS_STATES).get(&handle).cloned();
    if let Some(state) = state {
        let mut st = lock(&state);
        st.damage.add_all();
        st.needs_redraw = true;
    }
    FrameScheduler::schedule_canvas(handle as *mut c_void);
    unsafe {
        ng_platform_canvas_invalidate(handle as *mut c_void);
    }
}

pub(super) struct CanvasCleanup {
    pub(super) handle: usize,
    pub(super) renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
}

impl Drop for CanvasCleanup {
    fn drop(&mut self) {
        FrameScheduler::unregister_canvas(self.handle as *mut c_void);
        unregister_canvas_state(self.handle);
        let mut r = lock(&self.renderer);
        if let Some(ref mut renderer) = *r {
            renderer.cleanup();
        }
    }
}

#[cfg(feature = "zengpu")]
pub(super) fn ensure_canvas_renderer(
    handle: *mut c_void,
    state: &Arc<Mutex<CanvasState>>,
    renderer: &Arc<Mutex<Option<Box<dyn Renderer>>>>,
    backend: RendererBackend,
) -> AureaResult<bool> {
    if lock(renderer).is_some() {
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
        let st = lock(state);
        (st.width.max(1), st.height.max(1), st.scale_factor.max(1.0))
    };
    let gpu = aurea_render::ZenGpuRenderer::new(&handles, width, height, scale_factor)?;
    *lock(renderer) = Some(Box::new(gpu));
    Ok(true)
}

#[cfg(not(feature = "zengpu"))]
pub(super) fn ensure_canvas_renderer(
    _handle: *mut c_void,
    _state: &Arc<Mutex<CanvasState>>,
    renderer: &Arc<Mutex<Option<Box<dyn Renderer>>>>,
    _backend: RendererBackend,
) -> AureaResult<bool> {
    Ok(lock(renderer).is_some())
}

#[cfg(feature = "zengpu")]
fn zengpu_canvas_handles(handle: *mut c_void) -> AureaResult<zengpu_hal::WindowHandles> {
    #[cfg(target_os = "macos")]
    {
        let view = unsafe { ng_platform_canvas_get_native_handle(handle) };
        let native = crate::platform::handles::NativeWindowHandle::MacOS { ns_view: view };
        crate::platform::zengpu::window_handles(&native)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let native = crate::platform::handles::native_handle_from_canvas_ptr(handle)
            .ok_or(AureaError::ElementOperationFailed)?;
        crate::platform::zengpu::window_handles(&native)
    }
}
