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
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
#[cfg(feature = "zengpu")]
use zengpu_hal::WindowHandles;

use super::DrawCallback;

/// All per-frame mutable canvas properties in one lock.
/// Renderer lives in a separate Arc<Mutex<>> so render_frame can release this
/// lock before invoking the draw callback.
pub(crate) struct CanvasState {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub damage: DamageRegion,
    pub background_color: Color,
    pub draw_callback: Option<DrawCallback>,
    pub needs_redraw: bool,
    /// Physical-pixel damage rect from the previous rendered frame. Used to
    /// compute how much of the IOSurface double-buffer needs refreshing this
    /// frame (back surface is always 2 frames stale, so we union N and N-1).
    pub prev_frame_damage: Option<Rect>,
}

/// A canvas's identity, for as long as the process runs.
///
/// Allocated when the canvas is created and never given out again. The native
/// handle used to be the identity, and the platform is free to hand the same
/// address back for the next canvas — so work queued against a canvas that
/// has since gone would have redrawn whichever one took its place.
///
/// `Send + Sync`, whereas [`Canvas`](super::Canvas) is not, so this is what a
/// background callback holds to ask a canvas to redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanvasId(u64);

impl CanvasId {
    /// The raw value, for logging or as a key of the caller's own.
    pub fn get(self) -> u64 {
        self.0
    }
}

static NEXT_CANVAS_ID: AtomicU64 = AtomicU64::new(1);

/// What a live canvas needs to be reached by id.
struct CanvasEntry {
    state: Arc<Mutex<CanvasState>>,
    /// The native handle, which the scheduler and the platform still speak in.
    handle: usize,
}

static CANVAS_STATES: LazyLock<Mutex<HashMap<CanvasId, CanvasEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Takes the next id for a canvas that is about to exist.
pub(super) fn next_canvas_id() -> CanvasId {
    CanvasId(NEXT_CANVAS_ID.fetch_add(1, Ordering::Relaxed))
}

pub(super) fn register_canvas_state(id: CanvasId, handle: usize, state: Arc<Mutex<CanvasState>>) {
    lock(&CANVAS_STATES).insert(id, CanvasEntry { state, handle });
}

fn unregister_canvas_state(id: CanvasId) {
    lock(&CANVAS_STATES).remove(&id);
}

/// Request a full redraw of a canvas identified by its raw handle.
///
/// This is the `Send + Sync`-safe equivalent of [`Canvas::invalidate_all`](super::Canvas::invalidate_all): it
/// marks the canvas dirty so the frame scheduler actually **re-runs the draw
/// callback**, schedules a frame, then triggers a platform repaint.
///
/// Prefer this over calling the raw `ng_platform_canvas_invalidate` FFI from a
/// background callback: that FFI only re-blits the *cached* pixel buffer and
/// never sets `needs_redraw`, so the scheduler's redraw gate skips the draw
/// callback. Immediate-mode UIs that mutate draw state in response to input
/// (consuming a pending click, updating hover, etc.) need the callback to run,
/// which is exactly what this provides.
///
/// `id` comes from [`Canvas::id`](super::Canvas::id). It is a no-op if the
/// canvas has been dropped, and because ids are never reused it cannot reach
/// whichever canvas came afterwards.
pub fn request_canvas_redraw(id: CanvasId) {
    // Bail out before touching the scheduler or the FFI: an unknown id belongs
    // to a canvas that is already gone, and passing its stale address back
    // into native code is exactly what the no-op contract rules out.
    let Some((state, handle)) = lock(&CANVAS_STATES)
        .get(&id)
        .map(|entry| (entry.state.clone(), entry.handle))
    else {
        return;
    };
    {
        let mut st = lock(&state);
        st.damage.add_all();
        st.needs_redraw = true;
    }
    FrameScheduler::schedule_canvas(handle as *mut c_void);
    unsafe {
        ng_platform_canvas_invalidate(handle as *mut c_void);
    }
}

/// Unregisters the canvas from the scheduler, tears down the renderer and
/// destroys the native canvas when the *last* `Canvas` clone is dropped.
pub(super) struct CanvasCleanup {
    pub(super) handle: usize,
    /// This canvas's identity, which is how the registry knows it.
    pub(super) id: CanvasId,
    pub(super) renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
    /// False once the canvas has been added to a container, which frees it.
    pub(super) owns_native: AtomicBool,
}

impl Drop for CanvasCleanup {
    fn drop(&mut self) {
        FrameScheduler::unregister_canvas(self.handle as *mut c_void);
        unregister_canvas_state(self.id);
        {
            let mut r = lock(&self.renderer);
            if let Some(ref mut renderer) = *r {
                renderer.cleanup();
            }
        }
        // The native canvas used to outlive every Canvas that referred to it:
        // the scheduler and the renderer were cleaned up and the platform
        // object was left behind.
        if self.owns_native.load(Ordering::Acquire) {
            unsafe { ng_platform_destroy_element(self.handle as *mut c_void) };
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
    let gpu = ZenGpuRenderer::new(&handles, width, height, scale_factor)?;
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

/// Native handle extraction differs from [`native_handle_from_canvas_ptr`](crate::platform::handles::native_handle_from_canvas_ptr) on
/// macOS: the ZenGPU surface needs the resolved `NSView` from
/// `ng_platform_canvas_get_native_handle`, not the raw canvas handle.
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

    /// Ids are never handed out twice, so work queued against a canvas that
    /// has gone cannot reach whichever canvas took its place.
    #[test]
    fn ids_are_not_reused() {
        let first = next_canvas_id();
        let second = next_canvas_id();

        assert_ne!(first, second);
        assert!(second.get() > first.get());
    }

    /// Asking a canvas that no longer exists to redraw does nothing, rather
    /// than reaching into the platform with an address it no longer owns.
    #[test]
    fn redrawing_an_unknown_canvas_is_a_no_op() {
        request_canvas_redraw(next_canvas_id());
    }
}
