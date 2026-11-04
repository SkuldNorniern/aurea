//! Frame queue for scheduling and processing redraws.

use aurea_foundation::{AureaError, lock};
use std::cell::{Cell, RefCell};
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

/// A gap longer than this is idle time or a hang, not a frame rate.
const MAX_FRAME_GAP: f64 = 0.1;
/// How far a gap can be from the frame period and still count as on time.
const ON_TIME: f64 = 0.25;
/// Late or early frames in a row that mean the rate changed.
const NEW_RATE: u32 = 3;

/// Frame times with the timer's jitter taken out, so motion doesn't wobble: frames reach the screen
/// on a steady beat, whenever their timer happened to fire.
#[derive(Default)]
struct FrameClock {
    /// The steady time of the last frame, and when it really ran.
    last: Option<(Instant, Instant)>,
    period: Option<f64>,
    off_beat: u32,
}

impl FrameClock {
    fn advance(&mut self, now: Instant) -> (Instant, Duration) {
        let Some((time, real)) = self.last else {
            self.last = Some((now, now));
            return (now, Duration::ZERO);
        };
        let gap = now.saturating_duration_since(real).as_secs_f64();
        let elapsed = now.saturating_duration_since(time);
        if gap > MAX_FRAME_GAP {
            self.period = None;
            self.off_beat = 0;
            self.last = Some((now, now));
            return (now, elapsed);
        }
        let period = self.period.unwrap_or(gap);
        if (gap - period).abs() > period * ON_TIME {
            self.off_beat += 1;
            self.period = Some(if self.off_beat >= NEW_RATE {
                gap
            } else {
                period
            });
            self.last = Some((now, now));
            return (now, elapsed);
        }
        self.off_beat = 0;
        let period = period + (gap - period) / 16.0;
        self.period = Some(period);
        let behind = elapsed.as_secs_f64() - period;
        if behind.abs() > period / 2.0 {
            self.last = Some((now, now));
            return (now, elapsed);
        }
        let step = Duration::from_secs_f64(period + behind / 4.0);
        let steady = time + step;
        self.last = Some((steady, now));
        (steady, step)
    }
}

/// Handle for a per-frame callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameCallbackId(u64);

/// Handle for a registered ticker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TickerId(u64);

/// Time information passed to every ticker each frame.
#[derive(Debug, Clone, Copy)]
pub struct FrameInfo {
    /// When the frame started, on a steady beat: it can differ from the clock by a little jitter.
    pub time: Instant,
    /// Time since the previous frame, or since the ticker was registered on its first frame.
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
static CLOCK: LazyLock<Mutex<FrameClock>> = LazyLock::new(|| Mutex::new(FrameClock::default()));
static REQUEST_FRAME_HOOK: LazyLock<Mutex<RequestFrameHook>> = LazyLock::new(|| Mutex::new(None));

thread_local! {
    static CANVAS_REGISTRY: RefCell<HashMap<usize, CanvasRedrawCallback>> =
        RefCell::new(HashMap::new());
    static FRAME_CALLBACKS: RefCell<HashMap<FrameCallbackId, FrameCallback>> =
        RefCell::new(HashMap::new());
    static TICKERS: RefCell<HashMap<TickerId, Ticker>> = RefCell::new(HashMap::new());
}

struct Ticker {
    tick: TickerFn,
    /// When it was registered, until its first frame.
    since: Rc<Cell<Option<Instant>>>,
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
        TICKERS.with(|t| {
            t.borrow_mut().insert(
                id,
                Ticker {
                    tick: Rc::new(RefCell::new(ticker)),
                    since: Rc::new(Cell::new(Some(Instant::now()))),
                },
            )
        });
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        Self::notify_platform();
        id
    }

    /// Drops the ticker `id`.
    pub fn unregister_ticker(id: TickerId) {
        TICKERS.with(|t| t.borrow_mut().remove(&id));
    }

    fn run_tickers(frame_info: FrameInfo) {
        let tickers: Vec<_> = TICKERS.with(|t| {
            t.borrow()
                .iter()
                .map(|(id, f)| (*id, Rc::clone(&f.tick), Rc::clone(&f.since)))
                .collect()
        });
        let mut to_remove = Vec::new();
        for (id, ticker_fn, since) in tickers {
            let mut info = frame_info;
            // The time before it existed may be seconds of idling.
            if let Some(since) = since.take() {
                info.delta = info.delta.min(info.time.saturating_duration_since(since));
            }
            let keep = (ticker_fn.borrow_mut())(info);
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

        let (time, delta) = lock(&CLOCK).advance(Instant::now());
        let frame = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
        let frame_info = FrameInfo { time, delta, frame };

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
    use std::thread::sleep;

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
        *lock(&CLOCK) = FrameClock::default();
        *lock(&REQUEST_FRAME_HOOK) = None;
    }

    fn handle(id: usize) -> *mut c_void {
        id as *mut c_void
    }

    /// Runs the clock over gaps in milliseconds; returns each step in milliseconds and how far the
    /// clock ended from real time.
    fn run_clock(gaps: &[f64]) -> (Vec<f64>, f64) {
        let start = Instant::now();
        let mut clock = FrameClock::default();
        clock.advance(start);
        let (mut real, mut time) = (start, start);
        let steps = gaps
            .iter()
            .map(|gap| {
                real += Duration::from_secs_f64(gap / 1000.0);
                let (at, step) = clock.advance(real);
                time = at;
                step.as_secs_f64() * 1000.0
            })
            .collect();
        let off = real.saturating_duration_since(time) + time.saturating_duration_since(real);
        (steps, off.as_secs_f64() * 1000.0)
    }

    #[test]
    fn jitter_stays_out_of_frame_times() {
        let gaps: Vec<f64> = (0..600)
            .map(|i| 16.667 + if i % 2 == 0 { 1.5 } else { -1.5 })
            .collect();
        let (steps, off) = run_clock(&gaps);
        for step in &steps[120..] {
            assert!((step - 16.667).abs() < 0.5, "step {step}");
        }
        assert!(off < 3.0, "ended {off} ms from real time");
    }

    #[test]
    fn slow_jitter_never_drifts_from_real_time() {
        let gaps: Vec<f64> = (0..2000)
            .map(|i| 16.667 + 2.0 * (f64::from(i) * 0.05).sin() + 0.02)
            .collect();
        let (_, off) = run_clock(&gaps);
        assert!(off < 8.4, "ended {off} ms from real time");
    }

    #[test]
    fn a_dropped_frame_keeps_the_rate_and_a_new_rate_is_learned() {
        let mut gaps = vec![16.667; 120];
        gaps.push(33.3);
        gaps.extend([16.667; 60]);
        gaps.extend([6.944; 60]);
        let (steps, _) = run_clock(&gaps);
        assert!((steps[120] - 33.3).abs() < 0.5, "step {}", steps[120]);
        assert!((steps[121] - 16.667).abs() < 0.1, "after {}", steps[121]);
        let last = steps[steps.len() - 1];
        assert!((last - 6.944).abs() < 0.3, "settled on {last}");
    }

    #[test]
    fn idle_time_is_not_smoothed_away() {
        let mut gaps = vec![16.667; 30];
        gaps.push(5000.0);
        gaps.extend([16.667; 30]);
        let (steps, off) = run_clock(&gaps);
        assert!((steps[30] - 5000.0).abs() < 1.0);
        assert!((steps[60] - 16.667).abs() < 0.5);
        assert!(off < 1.0);
    }

    #[test]
    fn a_new_ticker_does_not_get_the_idle_time_before_it() {
        let _guard = TestGuard::new();
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        FrameScheduler::process_frames().expect("process");
        sleep(Duration::from_millis(120));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let theirs = Rc::clone(&seen);
        // Registered late, as an animation starting after a while with nothing to draw.
        let id = FrameScheduler::register_ticker(move |info| {
            theirs.borrow_mut().push(info.delta);
            true
        });
        FrameScheduler::process_frames().expect("process");
        sleep(Duration::from_millis(5));
        FrameScheduler::process_frames().expect("process");
        FrameScheduler::unregister_ticker(id);
        let seen = seen.borrow();
        assert!(seen[0] < Duration::from_millis(50), "first {:?}", seen[0]);
        assert!(seen[1] >= Duration::from_millis(5), "second {:?}", seen[1]);
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
