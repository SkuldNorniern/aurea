//! A canvas from creation to drop: its id, the registry background threads reach it through, and
//! the cleanup when the last handle goes.

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
