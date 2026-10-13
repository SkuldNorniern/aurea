//! A canvas from creation to drop: its id, the registry background threads reach it through, and
//! the cleanup when the last handle goes.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::os::raw::c_void;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, LazyLock, Mutex, Once, PoisonError};
use std::thread;
use std::time::Instant;

use aurea_foundation::lock;
use aurea_render::Renderer;
use aurea_runtime::FrameScheduler;

use super::state::CanvasId;
use crate::ffi::*;

static NEXT_CANVAS_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static NATIVE_CANVASES: RefCell<HashMap<usize, Weak<CanvasCleanup>>> = RefCell::new(HashMap::new());
}

pub(super) fn register_native_canvas(cleanup: &Rc<CanvasCleanup>) {
    NATIVE_CANVASES.with(|registry| {
        registry
            .borrow_mut()
            .insert(cleanup.handle, Rc::downgrade(cleanup));
    });
}

pub(super) fn canvas_is_live(handle: *mut c_void) -> bool {
    NATIVE_CANVASES.with(|registry| {
        registry
            .borrow()
            .get(&(handle as usize))
            .and_then(Weak::upgrade)
            .is_some_and(|cleanup| cleanup.live.load(Ordering::Acquire))
    })
}

pub(crate) fn notify_canvas_destroyed(handle: *mut c_void) {
    let cleanup = NATIVE_CANVASES.with(|registry| {
        registry
            .borrow_mut()
            .remove(&(handle as usize))
            .and_then(|weak| weak.upgrade())
    });
    if let Some(cleanup) = cleanup {
        cleanup.live.store(false, Ordering::Release);
        cleanup.owns_native.store(false, Ordering::Release);
        FrameScheduler::unregister_canvas(handle);
        unregister_canvas_state(cleanup.id);
        if let Ok(mut renderer) = cleanup.renderer.try_borrow_mut() {
            *renderer = None;
        }
    }
}

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

#[derive(Default)]
struct Deadlines {
    heap: BinaryHeap<Reverse<(Instant, CanvasId)>>,
}

impl Deadlines {
    fn insert(&mut self, at: Instant, id: CanvasId) {
        self.heap.push(Reverse((at, id)));
    }

    fn earliest(&self) -> Option<Instant> {
        self.heap.peek().map(|Reverse((at, _))| *at)
    }

    fn take_due(&mut self, now: Instant) -> Vec<CanvasId> {
        let mut due = Vec::new();
        while self.earliest().is_some_and(|at| at <= now) {
            if let Some(Reverse((_, id))) = self.heap.pop() {
                due.push(id);
            }
        }
        due
    }

    fn forget(&mut self, id: CanvasId) {
        self.heap.retain(|Reverse((_, queued))| *queued != id);
    }

    #[cfg(test)]
    fn queued(&self, id: CanvasId) -> bool {
        self.heap.iter().any(|Reverse((_, queued))| *queued == id)
    }
}

static CANVAS_DEADLINES: LazyLock<(Mutex<Deadlines>, Condvar)> =
    LazyLock::new(|| (Mutex::new(Deadlines::default()), Condvar::new()));

static DEADLINE_THREAD: Once = Once::new();

fn unregister_canvas_state(id: CanvasId) {
    lock(&CANVAS_HANDLES).remove(&id);
    lock(&FULL_REDRAW_REQUESTS).remove(&id);
    lock(&CANVAS_DEADLINES.0).forget(id);
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

/// Request a redraw of a canvas at `at`, or as soon as possible if `at` has already passed.
pub fn request_canvas_redraw_at(id: CanvasId, at: Instant) {
    if !lock(&CANVAS_HANDLES).contains_key(&id) {
        return;
    }
    let (deadlines, wake) = &*CANVAS_DEADLINES;
    lock(deadlines).insert(at, id);
    wake.notify_one();
    DEADLINE_THREAD.call_once(spawn_deadline_thread);
}

fn spawn_deadline_thread() {
    let spawned = thread::Builder::new()
        .name("aurea-canvas-deadlines".into())
        .spawn(run_deadlines);
    if let Err(e) = spawned {
        log::warn!("Could not start the canvas timer thread: {e}");
    }
}

fn run_deadlines() {
    let (deadlines, wake) = &*CANVAS_DEADLINES;
    let mut pending = lock(deadlines);
    loop {
        let now = Instant::now();
        let due = pending.take_due(now);
        if !due.is_empty() {
            drop(pending);
            for id in due {
                request_canvas_redraw(id);
            }
            pending = lock(deadlines);
            continue;
        }
        pending = match pending.earliest() {
            Some(at) => {
                wake.wait_timeout(pending, at.saturating_duration_since(now))
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
            None => wake.wait(pending).unwrap_or_else(PoisonError::into_inner),
        };
    }
}

pub(super) struct CanvasCleanup {
    pub(super) handle: usize,
    pub(super) id: CanvasId,
    pub(super) renderer: Rc<RefCell<Option<Box<dyn Renderer>>>>,
    pub(super) owns_native: AtomicBool,
    pub(super) live: AtomicBool,
    pub(super) warned_about_mixed_drawing: AtomicBool,
}

impl Drop for CanvasCleanup {
    fn drop(&mut self) {
        NATIVE_CANVASES.with(|registry| {
            registry.borrow_mut().remove(&self.handle);
        });
        self.live.store(false, Ordering::Release);
        FrameScheduler::unregister_canvas(self.handle as *mut c_void);
        unregister_canvas_state(self.id);
        {
            let renderer = self.renderer.borrow_mut().take();
            if let Some(mut renderer) = renderer {
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

#[cfg(test)]
mod deadline_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_past_deadline_is_due_immediately() {
        let now = Instant::now();
        let id = next_canvas_id();
        let mut deadlines = Deadlines::default();
        deadlines.insert(now - Duration::from_secs(1), id);

        assert_eq!(deadlines.take_due(now), vec![id]);
        assert_eq!(deadlines.earliest(), None);
    }

    #[test]
    fn a_future_deadline_is_not_due_yet() {
        let now = Instant::now();
        let at = now + Duration::from_secs(3600);
        let id = next_canvas_id();
        let mut deadlines = Deadlines::default();
        deadlines.insert(at, id);

        assert!(deadlines.take_due(now).is_empty());
        assert_eq!(deadlines.earliest(), Some(at));
    }

    #[test]
    fn several_deadlines_fire_in_time_order() {
        let base = Instant::now();
        let late = next_canvas_id();
        let early = next_canvas_id();
        let middle = next_canvas_id();
        let mut deadlines = Deadlines::default();
        deadlines.insert(base + Duration::from_secs(3), late);
        deadlines.insert(base + Duration::from_secs(1), early);
        deadlines.insert(base + Duration::from_secs(2), middle);
        deadlines.insert(base + Duration::from_secs(2), early);

        assert_eq!(deadlines.earliest(), Some(base + Duration::from_secs(1)));
        assert_eq!(
            deadlines.take_due(base + Duration::from_secs(2)),
            vec![early, early, middle]
        );
        assert_eq!(
            deadlines.take_due(base + Duration::from_secs(10)),
            vec![late]
        );
    }

    #[test]
    fn a_dropped_canvas_deadline_is_ignored() {
        let now = Instant::now();
        let gone = next_canvas_id();
        let mut deadlines = Deadlines::default();
        deadlines.insert(now, gone);

        for id in deadlines.take_due(now) {
            request_canvas_redraw(id);
        }

        assert!(!lock(&FULL_REDRAW_REQUESTS).contains(&gone));
        assert_eq!(deadlines.earliest(), None);
    }

    #[test]
    fn scheduling_an_unknown_canvas_queues_nothing() {
        let gone = next_canvas_id();
        request_canvas_redraw_at(gone, Instant::now());

        assert!(!lock(&CANVAS_DEADLINES.0).queued(gone));
    }

    #[test]
    fn forgetting_a_canvas_keeps_the_others() {
        let now = Instant::now();
        let dropped = next_canvas_id();
        let kept = next_canvas_id();
        let mut deadlines = Deadlines::default();
        deadlines.insert(now + Duration::from_secs(1), dropped);
        deadlines.insert(now + Duration::from_secs(2), kept);

        deadlines.forget(dropped);

        assert!(!deadlines.queued(dropped));
        assert_eq!(deadlines.take_due(now + Duration::from_secs(5)), vec![kept]);
    }
}
