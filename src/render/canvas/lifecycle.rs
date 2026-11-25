//! A canvas from creation to drop: its id, the registry background threads reach it through, and
//! the cleanup when the last handle goes.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::io;
use std::os::raw::c_void;
use std::ptr::null;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, LazyLock, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use aurea_foundation::lock;
use aurea_render::Renderer;
use aurea_runtime::{FrameInfo, FrameScheduler};

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

/// Whether canvas `id` is still up. Judged by id: a native handle can be reused by a later canvas.
pub(super) fn canvas_id_is_live(id: CanvasId) -> bool {
    lock(&CANVAS_HANDLES).contains_key(&id)
}

/// Trims the renderer of every live canvas on this thread, between frames. A renderer busy
/// drawing is skipped.
pub(crate) fn trim_renderers() {
    let cleanups: Vec<Rc<CanvasCleanup>> = NATIVE_CANVASES.with(|registry| {
        registry
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .collect()
    });
    for cleanup in cleanups {
        if !cleanup.live.load(Ordering::Acquire) {
            continue;
        }
        if let Ok(mut renderer) = cleanup.renderer.try_borrow_mut()
            && let Some(renderer) = renderer.as_mut()
        {
            renderer.trim_memory();
        }
    }
}

/// Live canvases on this thread that sit in `window`.
fn canvases_in(window: *mut c_void) -> Vec<Rc<CanvasCleanup>> {
    let cleanups: Vec<Rc<CanvasCleanup>> = NATIVE_CANVASES.with(|registry| {
        registry
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .collect()
    });
    cleanups
        .into_iter()
        .filter(|c| c.live.load(Ordering::Acquire))
        .filter(|c| unsafe { ng_platform_canvas_get_window(c.handle as *mut c_void) } == window)
        .collect()
}

/// Lets go of the frames of `window`'s canvases while nothing shows them. The next frame draws
/// them whole.
pub(crate) fn release_frames_in(window: *mut c_void) {
    for cleanup in canvases_in(window) {
        if let Ok(mut renderer) = cleanup.renderer.try_borrow_mut()
            && let Some(renderer) = renderer.as_mut()
        {
            // SAFETY: a null buffer drops the canvas's pointer into the frame about to go.
            unsafe {
                ng_platform_canvas_update_buffer(cleanup.handle as *mut c_void, null(), 0, 0, 0)
            };
            renderer.release_frame();
        }
    }
}

/// Draws `window`'s canvases again, after [`release_frames_in`].
pub(crate) fn redraw_canvases_in(window: *mut c_void) {
    for cleanup in canvases_in(window) {
        request_canvas_redraw(cleanup.id);
    }
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

static FRAME_REDRAW_REQUESTS: LazyLock<Mutex<HashSet<CanvasId>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Canvases with no native view that a function wakes instead, such as the iOS host's screen.
#[cfg(any(target_os = "ios", target_os = "android"))]
static HOST_CANVASES: Mutex<Vec<HostCanvas>> = Mutex::new(Vec::new());

/// A host canvas's id and what wakes it.
#[cfg(any(target_os = "ios", target_os = "android"))]
type HostCanvas = (CanvasId, fn());

/// A canvas id whose redraw requests call `wake`, for a host that draws without a [`Canvas`].
///
/// [`Canvas`]: super::Canvas
#[cfg(any(target_os = "ios", target_os = "android"))]
pub(crate) fn register_host_canvas(wake: fn()) -> CanvasId {
    let id = next_canvas_id();
    lock(&HOST_CANVASES).push((id, wake));
    id
}

fn host_wake(id: CanvasId) -> Option<fn()> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        lock(&HOST_CANVASES)
            .iter()
            .find(|(host, _)| *host == id)
            .map(|(_, wake)| *wake)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let _ = id;
        None
    }
}

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

/// Deadlines closer than this are slept to, not waited for on the condvar.
const PRECISE_WAIT: Duration = Duration::from_millis(20);

static CANVAS_DEADLINES: LazyLock<(Mutex<Deadlines>, Condvar)> =
    LazyLock::new(|| (Mutex::new(Deadlines::default()), Condvar::new()));

static DEADLINE_THREAD: Mutex<bool> = Mutex::new(false);

fn unregister_canvas_state(id: CanvasId) {
    lock(&CANVAS_HANDLES).remove(&id);
    lock(&FULL_REDRAW_REQUESTS).remove(&id);
    lock(&FRAME_REDRAW_REQUESTS).remove(&id);
    lock(&CANVAS_DEADLINES.0).forget(id);
}

pub(super) fn take_full_redraw_request(id: CanvasId) -> bool {
    lock(&FULL_REDRAW_REQUESTS).remove(&id)
}

pub(super) fn take_frame_request(id: CanvasId) -> bool {
    lock(&FRAME_REDRAW_REQUESTS).remove(&id)
}

/// Request a full redraw of a canvas from anywhere.
pub fn request_canvas_redraw(id: CanvasId) {
    schedule_canvas(id, &FULL_REDRAW_REQUESTS);
}

/// Request a redraw that repaints only what the draw callback changed, since the CPU renderer
/// diffs each frame against the last one.
pub(super) fn request_canvas_frame(id: CanvasId) {
    schedule_canvas(id, &FRAME_REDRAW_REQUESTS);
}

fn schedule_canvas(id: CanvasId, pending: &Mutex<HashSet<CanvasId>>) {
    let Some(handle) = lock(&CANVAS_HANDLES).get(&id).copied() else {
        if let Some(wake) = host_wake(id) {
            wake();
        }
        return;
    };
    lock(pending).insert(id);
    FrameScheduler::schedule_canvas(handle as *mut c_void);
}

/// The per-frame ticker of canvas `id`: it ends once the canvas is gone, and asks the canvas to
/// redraw after each tick. Judged by id, since a native handle can be reused by a later canvas.
pub(super) fn frame_ticker<F>(id: CanvasId, mut tick: F) -> impl FnMut(FrameInfo) -> bool
where
    F: FnMut(FrameInfo) -> bool,
{
    move |info| {
        if !lock(&CANVAS_HANDLES).contains_key(&id) {
            return false;
        }
        let keep = tick(info);
        request_canvas_frame(id);
        keep
    }
}

/// Request a redraw of a canvas at `at`, or as soon as possible if `at` has already passed.
pub fn request_canvas_redraw_at(id: CanvasId, at: Instant) {
    if !lock(&CANVAS_HANDLES).contains_key(&id) && host_wake(id).is_none() {
        return;
    }
    let (deadlines, wake) = &*CANVAS_DEADLINES;
    lock(deadlines).insert(at, id);
    wake.notify_one();
    ensure_deadline_thread(&DEADLINE_THREAD, spawn_deadline_thread);
}

/// Starts the deadline thread through `spawn` unless it runs. A failed start leaves it not running,
/// so the next request tries again.
fn ensure_deadline_thread(started: &Mutex<bool>, spawn: impl FnOnce() -> io::Result<()>) {
    let mut started = lock(started);
    if *started {
        return;
    }
    match spawn() {
        Ok(()) => *started = true,
        Err(e) => log::warn!("Could not start the canvas timer thread: {e}"),
    }
}

fn spawn_deadline_thread() -> io::Result<()> {
    thread::Builder::new()
        .name("aurea-canvas-deadlines".into())
        .spawn(run_deadlines)
        .map(drop)
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
            // a condvar timeout rounds up to the system tick, about 15.6 ms on windows, so a
            // 16 ms frame came at 31. a near deadline sleeps instead, std sleeps precisely
            Some(at) if at.saturating_duration_since(now) <= PRECISE_WAIT => {
                drop(pending);
                thread::sleep(at.saturating_duration_since(now));
                lock(deadlines)
            }
            Some(at) => {
                let early = at.saturating_duration_since(now) - PRECISE_WAIT;
                wake.wait_timeout(pending, early)
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
        // a handle can be reused once its view is gone, so only this canvas's own entries go
        let own = NATIVE_CANVASES.with(|registry| {
            let mut registry = registry.borrow_mut();
            let dead = registry
                .get(&self.handle)
                .is_some_and(|weak| weak.strong_count() == 0);
            if dead {
                registry.remove(&self.handle);
            }
            dead
        });
        self.live.store(false, Ordering::Release);
        if own {
            FrameScheduler::unregister_canvas(self.handle as *mut c_void);
        }
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
    use std::cell::Cell;

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

    fn fake_canvas(id: CanvasId, handle: *mut c_void) -> Rc<CanvasCleanup> {
        let cleanup = Rc::new(CanvasCleanup {
            handle: handle as usize,
            id,
            renderer: Rc::default(),
            owns_native: AtomicBool::new(false),
            live: AtomicBool::new(true),
            warned_about_mixed_drawing: AtomicBool::new(false),
        });
        register_native_canvas(&cleanup);
        register_canvas_state(id, handle as usize);
        cleanup
    }

    #[test]
    fn a_ticker_ends_with_its_canvas_even_if_the_handle_is_reused() {
        let handle = 0x2000 as *mut c_void;
        let old = fake_canvas(next_canvas_id(), handle);
        let calls = Rc::new(Cell::new(0));
        let counted = Rc::clone(&calls);
        let mut ticker = frame_ticker(old.id, move |_| {
            counted.set(counted.get() + 1);
            true
        });
        let info = FrameInfo {
            time: Instant::now(),
            delta: Duration::ZERO,
            frame: 0,
        };
        assert!(ticker(info));
        drop(old);
        let _next = fake_canvas(next_canvas_id(), handle);

        assert!(!ticker(info), "the dropped canvas's ticker still runs");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn a_dropped_canvas_does_not_take_down_a_reused_handle() {
        let handle = 0x2001 as *mut c_void;
        let old = fake_canvas(next_canvas_id(), handle);
        let old_id = old.id;
        notify_canvas_destroyed(handle);
        let new = fake_canvas(next_canvas_id(), handle);
        drop(old);

        let registered = NATIVE_CANVASES.with(|registry| {
            registry
                .borrow()
                .get(&(handle as usize))
                .is_some_and(|weak| weak.strong_count() > 0)
        });
        assert!(
            registered,
            "dropping the old canvas took the new one's entry"
        );
        assert!(canvas_id_is_live(new.id));
        assert!(!canvas_id_is_live(old_id));
    }
}

#[cfg(test)]
mod deadline_tests {
    use super::*;
    use std::cell::Cell;
    use std::time::Duration;

    #[test]
    fn a_failed_deadline_thread_start_is_tried_again() {
        let started = Mutex::new(false);
        let tries = Cell::new(0);
        ensure_deadline_thread(&started, || {
            tries.set(tries.get() + 1);
            Err(io::Error::other("no thread"))
        });
        ensure_deadline_thread(&started, || {
            tries.set(tries.get() + 1);
            Ok(())
        });
        ensure_deadline_thread(&started, || {
            tries.set(tries.get() + 1);
            Ok(())
        });
        assert_eq!(tries.get(), 2, "one failed start, one good one, then none");
    }

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

#[cfg(all(test, feature = "widgets", target_os = "windows"))]
mod native_lifetime_tests {
    //! Hidden native windows; kept in one process and on one UI thread.

    use crate::AureaError;
    use crate::elements::{Orientation, Stack};
    use crate::render::{Canvas, Color, RendererBackend};
    use crate::{Container, Label, Window};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn native_destruction_invalidates_all_canvas_handles() {
        let mut window = Window::new("canvas lifetime test", 160, 100).expect("window");
        let mut canvas = Canvas::new(32, 32, RendererBackend::Cpu).expect("canvas");
        let weak = canvas.downgrade();
        window.set_content(canvas.clone()).expect("content");
        pixel_reads_and_failed_frames_recover(&mut canvas);
        window
            .set_content(Label::new("replacement").expect("label"))
            .expect("replace");
        assert!(weak.upgrade().is_none());
        assert!(canvas.native_handle().is_null());
        assert!(canvas.draw(|_| Ok(())).is_err());
        canvas.invalidate_all();

        let mut nested = Canvas::new(32, 32, RendererBackend::Cpu).expect("nested");
        let weak = nested.downgrade();
        let mut stack = Stack::new(Orientation::Vertical).expect("stack");
        stack.add(nested.clone()).expect("add");
        window.set_content(stack).expect("stack content");
        drop(window);
        assert!(weak.upgrade().is_none());
        assert!(nested.draw(|_| Ok(())).is_err());

        let mut window = Window::new("canvas draw teardown test", 160, 100).expect("window");
        let mut canvas = Canvas::new(32, 32, RendererBackend::Cpu).expect("canvas");
        window.set_content(canvas.clone()).expect("content");
        let weak = canvas.downgrade();
        let result = canvas.draw(|_| window.set_content(Label::new("replacement")?));
        assert!(result.is_err());
        assert!(weak.upgrade().is_none());
        assert!(canvas.with_frame_pixels(|_, _, _| ()).is_none());

        super::minimize_tests::a_minimized_window_lets_go_of_its_frames_and_draws_them_again();
    }

    fn pixel_reads_and_failed_frames_recover(canvas: &mut Canvas) {
        canvas
            .draw(|ctx| ctx.clear(Color::rgb(255, 0, 0)))
            .expect("draw");
        canvas
            .with_frame_pixels(|pixels, _, _| {
                let nested = canvas
                    .with_frame_pixels(|inner, _, _| inner.as_ptr())
                    .expect("nested read");
                assert_eq!(
                    pixels.as_ptr(),
                    nested,
                    "both reads borrow the renderer's buffer"
                );
            })
            .expect("pixels");
        let attempts = Rc::new(Cell::new(0));
        let count = Rc::clone(&attempts);
        canvas
            .set_draw_callback(move |ctx| {
                count.set(count.get() + 1);
                if count.get() == 1 {
                    return Err(AureaError::RenderingFailed);
                }
                ctx.clear(Color::rgb(0, 255, 0))
            })
            .expect("callback");
        assert!(canvas.redraw_if_needed().is_err());
        canvas.redraw_if_needed().expect("retry");
        assert_eq!(attempts.get(), 2, "the failed redraw stays pending");
        let inner = canvas.downgrade();
        canvas
            .set_draw_callback(move |_| {
                let mut inner = inner.upgrade().expect("live");
                assert!(
                    inner.draw(|_| Ok(())).is_err(),
                    "recursive draw returns an error"
                );
                Ok(())
            })
            .expect("recursive callback");
        canvas.redraw_if_needed().expect("outer draw");
    }
}

#[cfg(all(
    test,
    feature = "widgets",
    any(target_os = "windows", target_os = "linux")
))]
mod minimize_tests {
    use crate::Window;
    use crate::lifecycle::{LifecycleEvent, invoke_lifecycle_callback};
    use crate::render::{Canvas, Color, RendererBackend};

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn minimizing_releases_frames() {
        a_minimized_window_lets_go_of_its_frames_and_draws_them_again();
    }

    pub(super) fn a_minimized_window_lets_go_of_its_frames_and_draws_them_again() {
        let mut window = Window::new("canvas minimize test", 160, 100).expect("window");
        let mut canvas = Canvas::new(32, 32, RendererBackend::Cpu).expect("canvas");
        window.set_content(canvas.clone()).expect("content");
        canvas
            .set_draw_callback(|ctx| ctx.clear(Color::rgb(255, 0, 0)))
            .expect("callback");
        canvas.redraw_if_needed().expect("draw");
        let len = |c: &Canvas| c.with_frame_pixels(|p, _, _| p.len()).unwrap_or(0);
        assert!(len(&canvas) > 0);

        invoke_lifecycle_callback(window.handle(), LifecycleEvent::WindowMinimized);
        assert_eq!(len(&canvas), 0, "the frame is gone while minimized");

        invoke_lifecycle_callback(window.handle(), LifecycleEvent::WindowRestored);
        canvas.redraw_if_needed().expect("redraw");
        let first = canvas.with_frame_pixels(|p, _, _| p[0]).expect("pixels");
        assert_eq!(first & 0x00FF_FFFF, 0x00FF_0000, "drawn whole again");
    }
}
