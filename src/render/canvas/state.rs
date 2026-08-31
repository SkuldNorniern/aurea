use std::cell::RefCell;
use std::rc::Rc;

#[cfg(feature = "zengpu")]
use crate::AureaError;
use crate::AureaResult;
use crate::ffi::*;
#[cfg(all(feature = "zengpu", target_os = "macos"))]
use crate::platform::handles::NativeWindowHandle;
#[cfg(feature = "zengpu")]
use crate::platform::handles::native_handle_from_canvas_ptr;
#[cfg(feature = "zengpu")]
use crate::platform::zengpu::window_handles;
use aurea_foundation::lock;
#[cfg(feature = "zengpu")]
use aurea_render::ZenGpuRenderer;
use aurea_render::{Color, Rect, Renderer, RendererBackend};
use aurea_runtime::{DamageRegion, FrameScheduler};
use std::collections::{HashMap, HashSet};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
#[cfg(feature = "zengpu")]
use zengpu_hal::WindowHandles;

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

/// A canvas's identity, for as long as the process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanvasId(u64);

impl CanvasId {
    /// The raw value, for logging or as a key of the caller's own.
    pub fn get(self) -> u64 {
        self.0
    }
}

static NEXT_CANVAS_ID: AtomicU64 = AtomicU64::new(1);

static CANVAS_HANDLES: LazyLock<Mutex<HashMap<CanvasId, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static FULL_REDRAW_REQUESTS: LazyLock<Mutex<HashSet<CanvasId>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub(super) fn next_canvas_id() -> CanvasId {
    CanvasId(NEXT_CANVAS_ID.fetch_add(1, Ordering::Relaxed))
}

pub(super) fn register_canvas_state(id: CanvasId, handle: usize) {
    lock(&CANVAS_HANDLES).insert(id, handle);
}

fn unregister_canvas_state(id: CanvasId) {
    lock(&CANVAS_HANDLES).remove(&id);
    lock(&FULL_REDRAW_REQUESTS).remove(&id);
}

pub(super) fn take_full_redraw_request(id: CanvasId) -> bool {
    lock(&FULL_REDRAW_REQUESTS).remove(&id)
}

/// Request a full redraw of a canvas from anywhere.
pub fn request_canvas_redraw(id: CanvasId) {
    let Some(handle) = lock(&CANVAS_HANDLES).get(&id).copied() else {
        return;
    };
    lock(&FULL_REDRAW_REQUESTS).insert(id);
    FrameScheduler::schedule_canvas(handle as *mut c_void);
}

pub(super) struct CanvasCleanup {
    pub(super) handle: usize,
    pub(super) id: CanvasId,
    pub(super) renderer: Rc<RefCell<Option<Box<dyn Renderer>>>>,
    pub(super) owns_native: AtomicBool,
    pub(super) warned_about_mixed_drawing: AtomicBool,
}

impl Drop for CanvasCleanup {
    fn drop(&mut self) {
        FrameScheduler::unregister_canvas(self.handle as *mut c_void);
        unregister_canvas_state(self.id);
        {
            let mut r = self.renderer.borrow_mut();
            if let Some(ref mut renderer) = *r {
                renderer.cleanup();
            }
        }
        if self.owns_native.load(Ordering::Acquire) {
            unsafe { ng_platform_destroy_element(self.handle as *mut c_void) };
        }
    }
}

#[cfg(feature = "zengpu")]
pub(super) fn ensure_canvas_renderer(
    handle: *mut c_void,
    state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
    backend: RendererBackend,
) -> AureaResult<bool> {
    if renderer.borrow_mut().is_some() {
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
        let st = state.borrow_mut();
        (st.width.max(1), st.height.max(1), st.scale_factor.max(1.0))
    };
    let gpu = ZenGpuRenderer::new(&handles, width, height, scale_factor)?;
    *renderer.borrow_mut() = Some(Box::new(gpu));
    Ok(true)
}

#[cfg(not(feature = "zengpu"))]
pub(super) fn ensure_canvas_renderer(
    _handle: *mut c_void,
    _state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
    _backend: RendererBackend,
) -> AureaResult<bool> {
    Ok(renderer.borrow_mut().is_some())
}

#[cfg(feature = "zengpu")]
fn zengpu_canvas_handles(handle: *mut c_void) -> AureaResult<WindowHandles> {
    #[cfg(target_os = "macos")]
    {
        let view = unsafe { ng_platform_canvas_get_native_handle(handle) };
        let native = NativeWindowHandle::MacOS { ns_view: view };
        window_handles(&native)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let native =
            native_handle_from_canvas_ptr(handle).ok_or(AureaError::ElementOperationFailed)?;
        window_handles(&native)
    }
}

#[cfg(test)]
mod id_tests {
    use super::*;

    #[test]
    fn ids_are_not_reused() {
        let first = next_canvas_id();
        let second = next_canvas_id();

        assert_ne!(first, second);
        assert!(second.get() > first.get());
    }

    #[test]
    fn redrawing_an_unknown_canvas_is_a_no_op() {
        request_canvas_redraw(next_canvas_id());
    }
}
