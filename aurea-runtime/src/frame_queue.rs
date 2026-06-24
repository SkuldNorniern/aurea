//! Frame queue for scheduling and processing redraws.

use aurea_foundation::{lock, AureaError};
use std::collections::{HashMap, HashSet};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

type CanvasRedrawCallback = Arc<dyn Fn() -> Result<(), AureaError> + Send + Sync>;
type FrameCallback = Arc<dyn Fn() + Send + Sync + 'static>;
type TickerFn = Arc<Mutex<dyn FnMut(FrameInfo) -> bool + Send>>;
type RequestFrameHook = Option<Box<dyn Fn() + Send + Sync>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameCallbackId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TickerId(u64);

/// Time information passed to every ticker each frame.
/// Sampled once by the scheduler so draw callbacks never read the wall clock.
#[derive(Debug, Clone, Copy)]
pub struct FrameInfo {
    pub time: Instant,
    pub delta: Duration,
    pub frame: u64,
}

static FRAME_SCHEDULED: AtomicBool = AtomicBool::new(false);
static ALL_CANVASES_SCHEDULED: AtomicBool = AtomicBool::new(false);
static CANVAS_REGISTRY: LazyLock<Mutex<Arc<HashMap<usize, CanvasRedrawCallback>>>> =
    LazyLock::new(|| Mutex::new(Arc::new(HashMap::new())));
static PENDING_CANVASES: LazyLock<Mutex<HashSet<usize>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static FRAME_CALLBACK_COUNTER: AtomicU64 = AtomicU64::new(0);
static FRAME_CALLBACKS: LazyLock<Mutex<Arc<HashMap<FrameCallbackId, FrameCallback>>>> =
    LazyLock::new(|| Mutex::new(Arc::new(HashMap::new())));
static TICKER_COUNTER: AtomicU64 = AtomicU64::new(0);
static TICKERS: LazyLock<Mutex<Arc<HashMap<TickerId, TickerFn>>>> =
    LazyLock::new(|| Mutex::new(Arc::new(HashMap::new())));
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);
static LAST_FRAME_TIME: LazyLock<Mutex<Instant>> = LazyLock::new(|| Mutex::new(Instant::now()));
static REQUEST_FRAME_HOOK: LazyLock<Mutex<RequestFrameHook>> = LazyLock::new(|| Mutex::new(None));

pub struct FrameScheduler;

impl FrameScheduler {
    pub fn set_request_frame_hook<F: Fn() + Send + Sync + 'static>(f: F) {
        *lock(&REQUEST_FRAME_HOOK) = Some(Box::new(f));
    }

    fn notify_platform() {
        if let Some(hook) = lock(&REQUEST_FRAME_HOOK).as_ref() {
            hook();
        }
    }

    pub fn schedule() {
        ALL_CANVASES_SCHEDULED.store(true, Ordering::Relaxed);
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        Self::notify_platform();
    }

    pub fn schedule_canvas(handle: *mut c_void) {
        let mut pending = lock(&PENDING_CANVASES);
        pending.insert(handle as usize);
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        drop(pending);
        Self::notify_platform();
    }

    pub fn take() -> bool {
        FRAME_SCHEDULED.swap(false, Ordering::Relaxed)
    }

    pub fn is_scheduled() -> bool {
        FRAME_SCHEDULED.load(Ordering::Relaxed)
    }

    pub fn register_canvas(handle: *mut c_void, callback: CanvasRedrawCallback) {
        let mut registry = lock(&CANVAS_REGISTRY);
        let mut updated = (**registry).clone();
        updated.insert(handle as usize, callback);
        *registry = Arc::new(updated);
    }

    pub fn unregister_canvas(handle: *mut c_void) {
        let mut registry = lock(&CANVAS_REGISTRY);
        let mut updated = (**registry).clone();
        updated.remove(&(handle as usize));
        *registry = Arc::new(updated);
        lock(&PENDING_CANVASES).remove(&(handle as usize));
    }

    pub fn register_frame_callback<F>(callback: F) -> FrameCallbackId
    where
        F: Fn() + Send + Sync + 'static,
    {
        let id = FrameCallbackId(FRAME_CALLBACK_COUNTER.fetch_add(1, Ordering::Relaxed));
        let mut callbacks = lock(&FRAME_CALLBACKS);
        let mut updated = (**callbacks).clone();
        updated.insert(id, Arc::new(callback));
        *callbacks = Arc::new(updated);
        id
    }

    pub fn unregister_frame_callback(id: FrameCallbackId) {
        let mut callbacks = lock(&FRAME_CALLBACKS);
        let mut updated = (**callbacks).clone();
        updated.remove(&id);
        *callbacks = Arc::new(updated);
    }

    /// Register a per-frame ticker. The closure receives [`FrameInfo`] every frame
    /// and must return `true` to keep running or `false` to unregister itself.
    /// Tickers run *before* canvas redraws so state mutations are visible in the
    /// same frame. Canvas-specific invalidation should call [`Self::schedule_canvas`]
    /// from inside the ticker.
    pub fn register_ticker<F>(ticker: F) -> TickerId
    where
        F: FnMut(FrameInfo) -> bool + Send + 'static,
    {
        let id = TickerId(TICKER_COUNTER.fetch_add(1, Ordering::Relaxed));
        let mut tickers = lock(&TICKERS);
        let mut updated = (**tickers).clone();
        updated.insert(id, Arc::new(Mutex::new(ticker)));
        *tickers = Arc::new(updated);
        // Pump-only arm: don't set ALL_CANVASES_SCHEDULED — one active ticker
        // must not force a full repaint of every canvas every frame.
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        Self::notify_platform();
        id
    }

    pub fn unregister_ticker(id: TickerId) {
        let mut tickers = lock(&TICKERS);
        let mut updated = (**tickers).clone();
        updated.remove(&id);
        *tickers = Arc::new(updated);
    }

    /// Runs every registered ticker once, unregistering any that return `false`.
    /// Locks are released before invoking user code: ticker callbacks may
    /// re-register canvases or other tickers.
    fn run_tickers(frame_info: FrameInfo) {
        let tickers = lock(&TICKERS).clone();
        let mut to_remove = Vec::new();
        for (id, ticker_fn) in tickers.iter() {
            let keep = {
                let mut f = ticker_fn.lock().expect("ticker mutex not poisoned");
                f(frame_info)
            };
            if !keep {
                to_remove.push(*id);
            }
        }
        for id in to_remove {
            Self::unregister_ticker(id);
        }
    }

    pub fn process_frames() -> Result<(), AureaError> {
        if !Self::take() {
            return Ok(());
        }

        // Sample frame time once — tickers receive it so draw callbacks never
        // read the wall clock themselves (required by the determinism contract).
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

        // === Tickers run before canvas redraws so mutations are visible this frame ===
        Self::run_tickers(frame_info);

        // === Canvas redraws ===
        Self::redraw_canvases();

        // Re-arm pump if tickers remain after removal (pump-only, not all-canvas).
        // Check the live map — not the snapshot — so finished tickers don't waste a frame.
        // scheduler.rs calls ng_platform_frame_idle() when !is_scheduled().
        if !lock(&TICKERS).is_empty() {
            FRAME_SCHEDULED.store(true, Ordering::Relaxed);
            Self::notify_platform();
        }

        Ok(())
    }

    /// Invokes either every registered canvas's redraw callback (full repaint)
    /// or just the ones pending a redraw, plus all global frame callbacks.
    /// Locks are released before invoking callbacks, which may re-register
    /// canvases or frame callbacks.
    fn redraw_canvases() {
        let process_all_canvases = ALL_CANVASES_SCHEDULED.swap(false, Ordering::Relaxed);
        let registry = lock(&CANVAS_REGISTRY).clone();
        let global_callbacks = lock(&FRAME_CALLBACKS).clone();

        let pending_handles = {
            let mut pending = lock(&PENDING_CANVASES);
            if process_all_canvases || pending.is_empty() {
                pending.clear();
                None
            } else {
                Some(pending.drain().collect::<Vec<_>>())
            }
        };

        match pending_handles {
            None => {
                for callback in registry.values() {
                    if let Err(e) = callback() {
                        log::warn!("Canvas redraw error: {:?}", e);
                    }
                }
            }
            Some(handles) => {
                for handle in handles {
                    if let Some(callback) = registry.get(&handle)
                        && let Err(e) = callback()
                    {
                        log::warn!("Canvas redraw error: {:?}", e);
                    }
                }
            }
        }

        for (_, callback) in global_callbacks.iter() {
            callback();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::LazyLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct TestGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
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
        *lock(&CANVAS_REGISTRY) = Arc::new(HashMap::new());
        lock(&PENDING_CANVASES).clear();
        *lock(&FRAME_CALLBACKS) = Arc::new(HashMap::new());
        *lock(&TICKERS) = Arc::new(HashMap::new());
        *lock(&LAST_FRAME_TIME) = Instant::now();
        *lock(&REQUEST_FRAME_HOOK) = None;
    }

    fn handle(id: usize) -> *mut c_void {
        id as *mut c_void
    }

    #[test]
    fn targeted_schedule_processes_only_pending_canvas() {
        let _guard = TestGuard::new();
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));

        let first_count = first.clone();
        FrameScheduler::register_canvas(
            handle(1),
            Arc::new(move || {
                first_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        let second_count = second.clone();
        FrameScheduler::register_canvas(
            handle(2),
            Arc::new(move || {
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
            Arc::new(move || {
                first_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        let second_count = second.clone();
        FrameScheduler::register_canvas(
            handle(4),
            Arc::new(move || {
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

        // Ticker returns true for the first two calls, then false.
        FrameScheduler::register_ticker(move |_info| {
            let n = c.fetch_add(1, Ordering::Relaxed);
            n < 2 // keep running while n was 0 or 1 (i.e. after 3rd call: n==2, return false)
        });

        for _ in 0..4 {
            FrameScheduler::schedule();
            FrameScheduler::process_frames().unwrap();
        }

        // Ticker must have been called exactly 3 times (n=0 → true, n=1 → true, n=2 → false).
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
