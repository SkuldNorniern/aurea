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

// Asking for a frame crosses threads; running one does not.
//
// A background thread may say "this canvas is dirty, wake up". Everything it
// touches to say so is below. What it must never touch is a callback, because
// running one means drawing, and drawing belongs to the UI thread.
static FRAME_SCHEDULED: AtomicBool = AtomicBool::new(false);
static ALL_CANVASES_SCHEDULED: AtomicBool = AtomicBool::new(false);
static PENDING_CANVASES: LazyLock<Mutex<HashSet<usize>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static FRAME_CALLBACK_COUNTER: AtomicU64 = AtomicU64::new(0);
static TICKER_COUNTER: AtomicU64 = AtomicU64::new(0);
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);
static LAST_FRAME_TIME: LazyLock<Mutex<Instant>> = LazyLock::new(|| Mutex::new(Instant::now()));
static REQUEST_FRAME_HOOK: LazyLock<Mutex<RequestFrameHook>> = LazyLock::new(|| Mutex::new(None));

// The callbacks themselves, owned by the thread that runs them.
//
// They were global, which meant every one of them had to be `Send + Sync`,
// which meant a draw callback could not capture anything UI-local and the
// canvas it drew had to be shared through a mutex it never needed. Held here
// instead, a callback is an ordinary closure on one thread.
thread_local! {
    static CANVAS_REGISTRY: RefCell<HashMap<usize, CanvasRedrawCallback>> =
        RefCell::new(HashMap::new());
    static FRAME_CALLBACKS: RefCell<HashMap<FrameCallbackId, FrameCallback>> =
        RefCell::new(HashMap::new());
    static TICKERS: RefCell<HashMap<TickerId, TickerFn>> = RefCell::new(HashMap::new());
}

pub struct FrameScheduler;

/// Which canvases a frame repaints.
///
/// Three answers, and they were previously two: an empty pending set was
/// treated as "everything", so a frame asked for by a ticker — which dirties
/// no canvas at all — repainted every canvas in the process, every frame.
enum Redraw {
    /// Nobody dirtied anything. The frame still runs tickers and callbacks.
    Nothing,
    /// These canvases were dirtied.
    These(Vec<usize>),
    /// Everything, because something asked for a full repaint.
    All,
}

impl FrameScheduler {
    pub fn set_request_frame_hook<F: Fn() + Send + Sync + 'static>(f: F) {
        *lock(&REQUEST_FRAME_HOOK) = Some(Box::new(f));
    }

    fn notify_platform() {
        if let Some(hook) = lock(&REQUEST_FRAME_HOOK).as_ref() {
            hook();
        }
    }

    /// Asks for a frame that repaints every canvas.
    ///
    /// For a change nothing can attribute to one canvas, such as a scale
    /// factor. To run a frame without repainting anything, use [`Self::wake`];
    /// to repaint one canvas, [`Self::schedule_canvas`].
    pub fn schedule() {
        ALL_CANVASES_SCHEDULED.store(true, Ordering::Relaxed);
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        Self::notify_platform();
    }

    /// Asks for a frame without dirtying anything.
    ///
    /// What a caller wants when the frame is the point and the pixels are
    /// not: work queued for a window, or a ticker that will decide for itself
    /// what to invalidate. Asking for a full repaint instead meant a single
    /// queued call repainted every canvas in the process.
    pub fn wake() {
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
        CANVAS_REGISTRY.with(|r| r.borrow_mut().insert(handle as usize, callback));
    }

    /// The canvases with a redraw callback on this thread.
    pub fn registered_canvases() -> Vec<*mut c_void> {
        CANVAS_REGISTRY.with(|r| r.borrow().keys().map(|&h| h as *mut c_void).collect())
    }

    pub fn unregister_canvas(handle: *mut c_void) {
        CANVAS_REGISTRY.with(|r| r.borrow_mut().remove(&(handle as usize)));
        lock(&PENDING_CANVASES).remove(&(handle as usize));
    }

    /// Registers a callback run once per frame, on the UI thread.
    ///
    /// It is held by the thread that will run it, so it may capture UI-local
    /// state and need not be `Send`.
    pub fn register_frame_callback<F>(callback: F) -> FrameCallbackId
    where
        F: Fn() + 'static,
    {
        let id = FrameCallbackId(FRAME_CALLBACK_COUNTER.fetch_add(1, Ordering::Relaxed));
        FRAME_CALLBACKS.with(|c| c.borrow_mut().insert(id, Rc::new(callback)));
        id
    }

    pub fn unregister_frame_callback(id: FrameCallbackId) {
        FRAME_CALLBACKS.with(|c| c.borrow_mut().remove(&id));
    }

    /// Register a per-frame ticker. The closure receives [`FrameInfo`] every frame
    /// and must return `true` to keep running or `false` to unregister itself.
    /// Tickers run *before* canvas redraws so state mutations are visible in the
    /// same frame. Canvas-specific invalidation should call [`Self::schedule_canvas`]
    /// from inside the ticker.
    pub fn register_ticker<F>(ticker: F) -> TickerId
    where
        F: FnMut(FrameInfo) -> bool + 'static,
    {
        let id = TickerId(TICKER_COUNTER.fetch_add(1, Ordering::Relaxed));
        TICKERS.with(|t| t.borrow_mut().insert(id, Rc::new(RefCell::new(ticker))));
        // Pump-only arm: don't set ALL_CANVASES_SCHEDULED — one active ticker
        // must not force a full repaint of every canvas every frame.
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        Self::notify_platform();
        id
    }

    pub fn unregister_ticker(id: TickerId) {
        TICKERS.with(|t| t.borrow_mut().remove(&id));
    }

    /// Runs every registered ticker once, unregistering any that return `false`.
    /// Locks are released before invoking user code: ticker callbacks may
    /// re-register canvases or other tickers.
    fn run_tickers(frame_info: FrameInfo) {
        // Snapshot first: a ticker may register or drop another one, and the
        // registry cannot be borrowed while user code runs.
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
        if TICKERS.with(|t| !t.borrow().is_empty()) {
            FRAME_SCHEDULED.store(true, Ordering::Relaxed);
            Self::notify_platform();
        }

        Ok(())
    }

    /// Runs the redraw callbacks `redraw` names.
    fn run_redraws(registry: &HashMap<usize, CanvasRedrawCallback>, redraw: Redraw) {
        let callbacks: Vec<&CanvasRedrawCallback> = match redraw {
            // A frame was asked for, but no canvas was dirtied. Something
            // else wanted the pump: a ticker, or work queued for a window.
            Redraw::Nothing => return,
            Redraw::All => registry.values().collect(),
            Redraw::These(handles) => handles
                .into_iter()
                .filter_map(|handle| registry.get(&handle))
                .collect(),
        };
        for callback in callbacks {
            if let Err(e) = callback() {
                log::warn!("Canvas redraw error: {:?}", e);
            }
        }
    }

    /// Invokes either every registered canvas's redraw callback (full repaint)
    /// or just the ones pending a redraw, plus all global frame callbacks.
    /// Locks are released before invoking callbacks, which may re-register
    /// canvases or frame callbacks.
    fn redraw_canvases() {
        let process_all_canvases = ALL_CANVASES_SCHEDULED.swap(false, Ordering::Relaxed);
        // Snapshots of the callbacks, so a redraw may register or drop one.
        let registry: HashMap<usize, CanvasRedrawCallback> =
            CANVAS_REGISTRY.with(|r| r.borrow().clone());
        let global_callbacks: Vec<FrameCallback> =
            FRAME_CALLBACKS.with(|c| c.borrow().values().cloned().collect());

        let redraw = {
            let mut pending = lock(&PENDING_CANVASES);
            if process_all_canvases {
                pending.clear();
                Redraw::All
            } else if pending.is_empty() {
                Redraw::Nothing
            } else {
                Redraw::These(pending.drain().collect::<Vec<_>>())
            }
        };

        Self::run_redraws(&registry, redraw);

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

    /// A ticker asks for a frame so its own callback runs. It must not drag
    /// every canvas into a repaint with it: the registration arm is careful
    /// not to set the redraw-everything flag, and an empty pending set used
    /// to mean the same thing anyway.
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

        // What a ticker does: ask for a frame, mark no canvas dirty.
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        FrameScheduler::process_frames().expect("process");

        assert_eq!(
            redraws.load(Ordering::Relaxed),
            0,
            "a canvas nobody dirtied was repainted anyway"
        );
    }

    /// A window finds its canvases here to redraw them when it is resized.
    #[test]
    fn registered_canvases_lists_what_can_redraw() {
        let _guard = TestGuard::new();
        FrameScheduler::register_canvas(handle(1), Rc::new(|| Ok(())));
        FrameScheduler::register_canvas(handle(2), Rc::new(|| Ok(())));
        FrameScheduler::unregister_canvas(handle(1));

        assert_eq!(FrameScheduler::registered_canvases(), vec![handle(2)]);
    }

    /// Dirtying one canvas redraws that one and leaves the others alone.
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

    /// Asking for everything still redraws everything.
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
