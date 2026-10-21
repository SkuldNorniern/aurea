//! Frame queue for scheduling and processing redraws.

use aurea_foundation::{AureaError, lock};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::raw::c_void;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

type CanvasRedrawCallback = Rc<dyn Fn() -> Result<(), AureaError>>;
type FrameCallback = Rc<dyn Fn() + 'static>;
type TickerFn = Rc<RefCell<dyn FnMut(FrameInfo) -> bool>>;
type RequestFrameHook = Option<Box<dyn Fn() + Send + Sync>>;

/// Handle for a per-frame callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameCallbackId(u64);

/// Handle for a registered ticker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TickerId(u64);

/// Time information passed to every ticker each frame.
#[derive(Debug, Clone, Copy)]
pub struct FrameInfo {
    /// When the frame started.
    pub time: Instant,
    /// Time since the previous frame.
    pub delta: Duration,
    /// Frame number since startup.
    pub frame: u64,
}

static FRAME_SCHEDULED: AtomicBool = AtomicBool::new(false);
static ALL_CANVASES_SCHEDULED: AtomicBool = AtomicBool::new(false);
static PENDING_CANVASES: LazyLock<Mutex<HashSet<usize>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static FRAME_CALLBACK_COUNTER: AtomicU64 = AtomicU64::new(0);
static TICKER_COUNTER: AtomicU64 = AtomicU64::new(0);
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);
static LAST_FRAME_TIME: LazyLock<Mutex<Instant>> = LazyLock::new(|| Mutex::new(Instant::now()));
static REQUEST_FRAME_HOOK: LazyLock<Mutex<RequestFrameHook>> = LazyLock::new(|| Mutex::new(None));

thread_local! {
    static CANVAS_REGISTRY: RefCell<HashMap<usize, CanvasRedrawCallback>> =
        RefCell::new(HashMap::new());
    static FRAME_CALLBACKS: RefCell<HashMap<FrameCallbackId, FrameCallback>> =
        RefCell::new(HashMap::new());
    static TICKERS: RefCell<HashMap<TickerId, TickerFn>> = RefCell::new(HashMap::new());
}

/// Requests frames and runs per-frame work.
pub struct FrameScheduler;

impl FrameScheduler {
    /// Sets the hook that wakes the platform when a frame is requested.
    pub fn set_request_frame_hook<F: Fn() + Send + Sync + 'static>(f: F) {
        *lock(&REQUEST_FRAME_HOOK) = Some(Box::new(f));
    }

    fn notify_platform() {
        if let Some(hook) = lock(&REQUEST_FRAME_HOOK).as_ref() {
            hook();
        }
    }

    /// Asks for a frame that repaints every canvas.
    pub fn schedule() {
        ALL_CANVASES_SCHEDULED.store(true, Ordering::Relaxed);
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        Self::notify_platform();
    }

    /// Asks for a frame without dirtying anything.
    pub fn wake() {
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        Self::notify_platform();
    }

    /// Asks for a frame that repaints the canvas `handle`.
    pub fn schedule_canvas(handle: *mut c_void) {
        let mut pending = lock(&PENDING_CANVASES);
        pending.insert(handle as usize);
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        drop(pending);
        Self::notify_platform();
    }

    /// Clears the scheduled flag, returning whether it was set.
    pub fn take() -> bool {
        FRAME_SCHEDULED.swap(false, Ordering::Relaxed)
    }

    /// Whether a frame is pending.
    pub fn is_scheduled() -> bool {
        FRAME_SCHEDULED.load(Ordering::Relaxed)
    }

    /// Registers the redraw callback for canvas `handle`.
    pub fn register_canvas(handle: *mut c_void, callback: CanvasRedrawCallback) {
        CANVAS_REGISTRY.with(|r| r.borrow_mut().insert(handle as usize, callback));
    }

    /// Drops the redraw callback and any pending redraw for canvas `handle`.
    pub fn unregister_canvas(handle: *mut c_void) {
        CANVAS_REGISTRY.with(|r| r.borrow_mut().remove(&(handle as usize)));
        lock(&PENDING_CANVASES).remove(&(handle as usize));
    }

    /// Registers a callback run once per frame, on the UI thread.
    pub fn register_frame_callback<F>(callback: F) -> FrameCallbackId
    where
        F: Fn() + 'static,
    {
        let id = FrameCallbackId(FRAME_CALLBACK_COUNTER.fetch_add(1, Ordering::Relaxed));
        FRAME_CALLBACKS.with(|c| c.borrow_mut().insert(id, Rc::new(callback)));
        id
    }

    /// Drops the per-frame callback `id`.
    pub fn unregister_frame_callback(id: FrameCallbackId) {
        FRAME_CALLBACKS.with(|c| c.borrow_mut().remove(&id));
    }

    /// Register a per-frame ticker.
    pub fn register_ticker<F>(ticker: F) -> TickerId
    where
        F: FnMut(FrameInfo) -> bool + 'static,
    {
        let id = TickerId(TICKER_COUNTER.fetch_add(1, Ordering::Relaxed));
        TICKERS.with(|t| t.borrow_mut().insert(id, Rc::new(RefCell::new(ticker))));
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        Self::notify_platform();
        id
    }

    /// Drops the ticker `id`.
    pub fn unregister_ticker(id: TickerId) {
        TICKERS.with(|t| t.borrow_mut().remove(&id));
    }

    fn run_tickers(frame_info: FrameInfo) {
        let tickers: Vec<(TickerId, TickerFn)> = TICKERS.with(|t| {
            t.borrow()
                .iter()
                .map(|(id, f)| (*id, Rc::clone(f)))
                .collect()
        });
        let mut to_remove = Vec::new();
        for (id, ticker_fn) in tickers {
            let keep = (ticker_fn.borrow_mut())(frame_info);
            if !keep {
                to_remove.push(id);
            }
        }
        for id in to_remove {
            Self::unregister_ticker(id);
        }
    }

    /// Runs one frame if one is scheduled.
    pub fn process_frames() -> Result<(), AureaError> {
        if !Self::take() {
            return Ok(());
        }

        let now = Instant::now();
        let delta = {
            let mut last = lock(&LAST_FRAME_TIME);
            let d = now.duration_since(*last);
            *last = now;
            d
        };
        let frame = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
        let frame_info = FrameInfo {
            time: now,
            delta,
            frame,
        };

        Self::run_tickers(frame_info);

        Self::redraw_canvases();

        if TICKERS.with(|t| !t.borrow().is_empty()) {
            FRAME_SCHEDULED.store(true, Ordering::Relaxed);
            Self::notify_platform();
        }

        Ok(())
    }

    fn run_redraws(callbacks: Vec<CanvasRedrawCallback>) {
        for callback in callbacks {
            if let Err(e) = callback() {
                log::warn!("Canvas redraw error: {:?}", e);
            }
        }
    }

    fn redraw_canvases() {
        let process_all_canvases = ALL_CANVASES_SCHEDULED.swap(false, Ordering::Relaxed);
        let callbacks = CANVAS_REGISTRY.with(|registry| {
            let registry = registry.borrow();
            let mut pending = lock(&PENDING_CANVASES);
            if process_all_canvases {
                pending.clear();
                drop(pending);
                registry.values().cloned().collect()
            } else {
                pending
                    .drain()
                    .filter_map(|handle| registry.get(&handle).cloned())
                    .collect()
            }
        });
        let global_callbacks: Vec<FrameCallback> =
            FRAME_CALLBACKS.with(|c| c.borrow().values().cloned().collect());

        Self::run_redraws(callbacks);

        for callback in global_callbacks {
            callback();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::LazyLock;
    use std::sync::MutexGuard;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct TestGuard {
        _guard: MutexGuard<'static, ()>,
    }

    impl TestGuard {
        fn new() -> Self {
            let guard = lock(&TEST_LOCK);
            reset_scheduler_state();
            Self { _guard: guard }
        }
    }

    impl Drop for TestGuard {
        fn drop(&mut self) {
            reset_scheduler_state();
        }
    }

    fn reset_scheduler_state() {
        FRAME_SCHEDULED.store(false, Ordering::Relaxed);
        ALL_CANVASES_SCHEDULED.store(false, Ordering::Relaxed);
        FRAME_CALLBACK_COUNTER.store(0, Ordering::Relaxed);
        TICKER_COUNTER.store(0, Ordering::Relaxed);
        FRAME_COUNTER.store(0, Ordering::Relaxed);
        CANVAS_REGISTRY.with(|r| r.borrow_mut().clear());
        lock(&PENDING_CANVASES).clear();
        FRAME_CALLBACKS.with(|c| c.borrow_mut().clear());
        TICKERS.with(|t| t.borrow_mut().clear());
        *lock(&LAST_FRAME_TIME) = Instant::now();
        *lock(&REQUEST_FRAME_HOOK) = None;
    }

    fn handle(id: usize) -> *mut c_void {
        id as *mut c_void
    }

    #[test]
    fn a_frame_with_nothing_dirty_redraws_no_canvas() {
        let _guard = TestGuard::new();

        let redraws = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&redraws);
        FrameScheduler::register_canvas(
            handle(1),
            Rc::new(move || {
                counted.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        FrameScheduler::process_frames().expect("process");

        assert_eq!(
            redraws.load(Ordering::Relaxed),
            0,
            "a canvas nobody dirtied was repainted anyway"
        );
    }

    #[test]
    fn only_the_dirty_canvas_is_redrawn() {
        let _guard = TestGuard::new();

        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));
        for (id, counter) in [(1usize, &first), (2usize, &second)] {
            let counted = Arc::clone(counter);
            FrameScheduler::register_canvas(
                handle(id),
                Rc::new(move || {
                    counted.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }),
            );
        }

        FrameScheduler::schedule_canvas(handle(1));
        FrameScheduler::process_frames().expect("process");

        assert_eq!(first.load(Ordering::Relaxed), 1);
        assert_eq!(second.load(Ordering::Relaxed), 0, "not dirty, not redrawn");
    }

    #[test]
    fn targeted_redraws_release_removed_idle_canvases_and_allow_rescheduling() {
        let _guard = TestGuard::new();
        let idle: CanvasRedrawCallback = Rc::new(|| Ok(()));
        let idle_owner = Rc::downgrade(&idle);
        FrameScheduler::register_canvas(handle(2), idle);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let first_seen = Rc::clone(&seen);
        FrameScheduler::register_canvas(
            handle(1),
            Rc::new(move || {
                first_seen.borrow_mut().push(1);
                FrameScheduler::unregister_canvas(handle(2));
                assert!(
                    idle_owner.upgrade().is_none(),
                    "idle canvas is not held by the frame snapshot"
                );
                FrameScheduler::unregister_canvas(handle(1));
                let next_seen = Rc::clone(&first_seen);
                FrameScheduler::register_canvas(
                    handle(3),
                    Rc::new(move || {
                        next_seen.borrow_mut().push(3);
                        Ok(())
                    }),
                );
                FrameScheduler::schedule_canvas(handle(3));
                Ok(())
            }),
        );
        FrameScheduler::schedule_canvas(handle(1));
        FrameScheduler::process_frames().expect("first frame");
        assert_eq!(*seen.borrow(), [1]);
        FrameScheduler::process_frames().expect("rescheduled frame");
        assert_eq!(*seen.borrow(), [1, 3]);
    }

    #[test]
    fn scheduling_all_redraws_every_canvas() {
        let _guard = TestGuard::new();

        let redraws = Arc::new(AtomicUsize::new(0));
        for id in 1..=2usize {
            let counted = Arc::clone(&redraws);
            FrameScheduler::register_canvas(
                handle(id),
                Rc::new(move || {
                    counted.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }),
            );
        }

        FrameScheduler::schedule();
        FrameScheduler::process_frames().expect("process");

        assert_eq!(redraws.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn targeted_schedule_processes_only_pending_canvas() {
        let _guard = TestGuard::new();
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));

        let first_count = first.clone();
        FrameScheduler::register_canvas(
            handle(1),
            Rc::new(move || {
                first_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        let second_count = second.clone();
        FrameScheduler::register_canvas(
            handle(2),
            Rc::new(move || {
                second_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        FrameScheduler::schedule_canvas(handle(1));
        FrameScheduler::process_frames().unwrap();

        assert_eq!(first.load(Ordering::Relaxed), 1);
        assert_eq!(second.load(Ordering::Relaxed), 0);

        FrameScheduler::unregister_canvas(handle(1));
        FrameScheduler::unregister_canvas(handle(2));
    }

    #[test]
    fn global_schedule_processes_all_canvases() {
        let _guard = TestGuard::new();
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));

        let first_count = first.clone();
        FrameScheduler::register_canvas(
            handle(3),
            Rc::new(move || {
                first_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        let second_count = second.clone();
        FrameScheduler::register_canvas(
            handle(4),
            Rc::new(move || {
                second_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        FrameScheduler::schedule_canvas(handle(3));
        FrameScheduler::schedule();
        FrameScheduler::process_frames().unwrap();

        assert_eq!(first.load(Ordering::Relaxed), 1);
        assert_eq!(second.load(Ordering::Relaxed), 1);

        FrameScheduler::unregister_canvas(handle(3));
        FrameScheduler::unregister_canvas(handle(4));
    }

    #[test]
    fn frame_callback_unregister_stops_invocation() {
        let _guard = TestGuard::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let id = FrameScheduler::register_frame_callback(move || {
            c.fetch_add(1, Ordering::Relaxed);
        });

        FrameScheduler::schedule();
        FrameScheduler::process_frames().unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 1);

        FrameScheduler::unregister_frame_callback(id);
        FrameScheduler::schedule();
        FrameScheduler::process_frames().unwrap();
        assert_eq!(
            count.load(Ordering::Relaxed),
            1,
            "callback must not fire after unregister"
        );
    }

    #[test]
    fn ticker_runs_until_false() {
        let _guard = TestGuard::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();

        FrameScheduler::register_ticker(move |_info| {
            let n = c.fetch_add(1, Ordering::Relaxed);
            n < 2
        });

        for _ in 0..4 {
            FrameScheduler::schedule();
            FrameScheduler::process_frames().unwrap();
        }

        assert_eq!(count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn ticker_explicit_unregister() {
        let _guard = TestGuard::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let id = FrameScheduler::register_ticker(move |_| {
            c.fetch_add(1, Ordering::Relaxed);
            true
        });

        FrameScheduler::schedule();
        FrameScheduler::process_frames().unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 1);

        FrameScheduler::unregister_ticker(id);
        FrameScheduler::schedule();
        FrameScheduler::process_frames().unwrap();
        assert_eq!(
            count.load(Ordering::Relaxed),
            1,
            "ticker must not fire after unregister"
        );
    }
}
