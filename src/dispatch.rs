//! Work handed to the UI thread from any other thread, without a window, and work the UI thread
//! puts off until the frame it is in has finished.

use std::cell::RefCell;
use std::mem::take;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use aurea_foundation::lock;
use aurea_runtime::FrameScheduler;

#[cfg(target_os = "android")]
use crate::android;
#[cfg(target_os = "ios")]
use crate::ios;
use crate::platform::ui_thread;
use crate::render::{CanvasId, request_canvas_redraw};

type Call = Box<dyn FnOnce() + Send>;
type LocalCall = Box<dyn FnOnce()>;

static QUEUE: Mutex<Vec<Call>> = Mutex::new(Vec::new());

thread_local! {
    static LATER: RefCell<Vec<LocalCall>> = const { RefCell::new(Vec::new()) };
}

/// Queues `call` to run on the UI thread, soon, whether a window is shown or not.
///
/// ```rust,no_run
/// std::thread::spawn(|| {
///     aurea::on_ui_thread(|| println!("on the UI thread"));
/// });
/// ```
pub fn on_ui_thread(call: impl FnOnce() + Send + 'static) {
    lock(&QUEUE).push(Box::new(call));
    wake_host();
}

/// Runs `call` on the UI thread after `delay`, from any thread, and wakes it.
pub fn run_after(delay: Duration, call: impl FnOnce() + Send + 'static) {
    thread::spawn(move || {
        thread::sleep(delay);
        on_ui_thread(call);
    });
}

/// Runs `call` on the UI thread once the frame being drawn has finished, with nothing borrowed: not
/// the canvas, not the renderer, not the state a draw callback holds.
///
/// ```rust,no_run
/// # use std::{cell::RefCell, rc::Rc};
/// # let app = Rc::new(RefCell::new(Vec::<std::path::PathBuf>::new()));
/// # let window = Rc::new(aurea::Window::new("App", 400, 300).unwrap());
/// // In a draw callback, when an "Open" button was clicked:
/// let (app, window) = (Rc::clone(&app), Rc::clone(&window));
/// aurea::after_frame(move || {
///     if let Ok(paths) = aurea::dialog::FileDialog::new().open(&window) {
///         app.borrow_mut().extend(paths);
///     }
/// });
/// ```
pub fn after_frame(call: impl FnOnce() + 'static) {
    ui_thread::check("after_frame");
    LATER.with(|later| later.borrow_mut().push(Box::new(call)));
    wake_host();
}

/// Asks for a frame, where the queue drains: the desktop loop, or the mobile host's next frame.
/// Wakes the UI from any thread, for a worker that has something new to draw.
#[derive(Clone, Copy, Debug, Default)]
pub struct WakeHandle {
    canvas: Option<CanvasId>,
}

impl WakeHandle {
    /// Asks for a frame, and a redraw of the canvas it was made for.
    pub fn wake(self) {
        if let Some(id) = self.canvas {
            request_canvas_redraw(id);
        }
        wake_host();
    }
}

/// A [`WakeHandle`] for a worker started before its canvas exists: it wakes the UI until
/// [`Self::set_canvas`], and redraws the canvas too from then on. Clones share the canvas.
#[derive(Clone, Debug, Default)]
pub struct LateWakeHandle {
    canvas: Arc<OnceLock<CanvasId>>,
}

impl LateWakeHandle {
    /// A handle with no canvas yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Names the canvas, once; a later call changes nothing.
    pub fn set_canvas(&self, id: CanvasId) {
        let _ = self.canvas.set(id);
    }

    /// Asks for a frame, and a redraw of the canvas when it has one.
    pub fn wake(&self) {
        WakeHandle {
            canvas: self.canvas.get().copied(),
        }
        .wake();
    }
}

/// A handle a worker can keep, on every host.
pub fn wake_handle() -> WakeHandle {
    WakeHandle::default()
}

/// A handle that also redraws `id` when woken, on every host.
pub fn wake_canvas(id: CanvasId) -> WakeHandle {
    WakeHandle { canvas: Some(id) }
}

fn wake_host() {
    FrameScheduler::wake();
    #[cfg(target_os = "ios")]
    ios::request_frame();
    #[cfg(target_os = "android")]
    android::request_frame();
}

pub(crate) fn drain() {
    let calls = take(&mut *lock(&QUEUE));
    for call in calls {
        call();
    }
    let later = LATER.with(|later| take(&mut *later.borrow_mut()));
    for call in later {
        call();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_QUEUE: Mutex<()> = Mutex::new(());

    #[test]
    fn a_late_handle_wakes_before_it_has_a_canvas() {
        let handle = LateWakeHandle::new();
        handle.clone().wake();
        assert!(handle.canvas.get().is_none());
    }

    #[test]
    fn queued_calls_run_once_in_order_and_may_queue_more() {
        let _serial = lock(&TEST_QUEUE);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let runs = Arc::new(AtomicUsize::new(0));
        for n in 0..3 {
            let seen = seen.clone();
            let runs = runs.clone();
            on_ui_thread(move || {
                runs.fetch_add(1, Ordering::SeqCst);
                lock(&seen).push(n);
                if n == 2 {
                    let seen = seen.clone();
                    on_ui_thread(move || lock(&seen).push(9));
                }
            });
        }
        drain();
        assert_eq!(*lock(&seen), [0, 1, 2]);
        drain();
        assert_eq!(*lock(&seen), [0, 1, 2, 9]);
        drain();
        assert_eq!(runs.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn wake_handles_are_send_sync_and_copy() {
        fn assert_handle<T: Send + Sync + Copy>() {}
        assert_handle::<WakeHandle>();
        let handle = wake_handle();
        assert!(handle.canvas.is_none());
        let copy = handle;
        copy.wake();
        handle.wake();
    }

    #[test]
    fn put_off_calls_run_at_the_next_drain_and_may_hold_rc() {
        let _serial = lock(&TEST_QUEUE);
        use std::rc::Rc;
        let seen = Rc::new(RefCell::new(Vec::new()));
        let held = Rc::clone(&seen);
        LATER.with(|later| {
            later.borrow_mut().push(Box::new(move || {
                held.borrow_mut().push(1);
                let again = Rc::clone(&held);
                LATER.with(|later| {
                    later
                        .borrow_mut()
                        .push(Box::new(move || again.borrow_mut().push(2)))
                });
            }));
        });
        assert!(seen.borrow().is_empty(), "not before the frame ends");
        drain();
        assert_eq!(*seen.borrow(), [1]);
        drain();
        assert_eq!(
            *seen.borrow(),
            [1, 2],
            "one queued while running waits a turn"
        );
    }
}
