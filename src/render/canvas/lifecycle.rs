//! A canvas from creation to drop: its id, the registry background threads
//! reach it through, and the cleanup when the last handle goes.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::raw::c_void;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use aurea_foundation::lock;
use aurea_render::Renderer;
use aurea_runtime::FrameScheduler;

use super::state::CanvasId;
use crate::ffi::*;

static NEXT_CANVAS_ID: AtomicU64 = AtomicU64::new(1);

/// What a background thread may know about a live canvas: where to send the
/// wake-up, and nothing else.
///
/// The canvas's own state stays with the UI thread. It holds the draw
/// callback, which draws, and a thread that is not going to draw has no
/// business reaching it.
static CANVAS_HANDLES: LazyLock<Mutex<HashMap<CanvasId, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Canvases somebody has asked to redraw in full, waiting for the UI thread
/// to notice.
static FULL_REDRAW_REQUESTS: LazyLock<Mutex<HashSet<CanvasId>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Takes the next id for a canvas that is about to exist.
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

/// Whether this canvas was asked to redraw in full since the last frame, and
/// forgets the request. For the UI thread, on its way into a frame.
pub(super) fn take_full_redraw_request(id: CanvasId) -> bool {
    lock(&FULL_REDRAW_REQUESTS).remove(&id)
}

/// Request a full redraw of a canvas from anywhere.
///
/// This is the `Send`-safe equivalent of
/// [`Canvas::invalidate_all`](super::Canvas::invalidate_all). It records the
/// request and asks for a frame; the UI thread marks the canvas dirty and
/// re-runs its draw callback when it gets there.
///
/// Nothing native happens here, and no canvas state is touched. GTK and
/// AppKit will not be driven from a background thread, and the draw callback
/// belongs to the UI thread that runs it.
///
/// `id` comes from [`Canvas::id`](super::Canvas::id). It is a no-op if the
/// canvas has been dropped, and because ids are never reused it cannot reach
/// whichever canvas came afterwards.
pub fn request_canvas_redraw(id: CanvasId) {
    let Some(handle) = lock(&CANVAS_HANDLES).get(&id).copied() else {
        return;
    };
    lock(&FULL_REDRAW_REQUESTS).insert(id);
    FrameScheduler::schedule_canvas(handle as *mut c_void);
}

/// Unregisters the canvas from the scheduler, tears down the renderer and
/// destroys the native canvas when the *last* `Canvas` clone is dropped.
pub(super) struct CanvasCleanup {
    pub(super) handle: usize,
    /// This canvas's identity, which is how the registry knows it.
    pub(super) id: CanvasId,
    pub(super) renderer: Rc<RefCell<Option<Box<dyn Renderer>>>>,
    /// False once the canvas has been added to a container, which frees it.
    pub(super) owns_native: AtomicBool,
    /// Whether the immediate/retained clash has already been reported, so a
    /// canvas drawing every frame says it once instead of filling the log.
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
        // The native canvas used to outlive every Canvas that referred to it:
        // the scheduler and the renderer were cleaned up and the platform
        // object was left behind.
        if self.owns_native.load(Ordering::Acquire) {
            unsafe { ng_platform_destroy_element(self.handle as *mut c_void) };
        }
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
