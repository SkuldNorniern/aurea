//! Work handed to the UI thread from any other thread, without a window,
//! and work the UI thread puts off until the frame it is in has finished.
//!
//! [`WindowProxy::dispatch`](crate::window::WindowProxy::dispatch) needs the
//! window, and only runs when that window is pumped by hand. Things that are
//! not windows, like a [`TrayIcon`](crate::TrayIcon), and apps that sit in
//! [`Window::run`](crate::Window::run), use this instead: the platform's own
//! pump runs the queue.

use std::cell::RefCell;
use std::mem::take;
use std::sync::Mutex;

use aurea_foundation::lock;
use aurea_runtime::FrameScheduler;

use crate::platform::ui_thread;

type Call = Box<dyn FnOnce() + Send>;
type LocalCall = Box<dyn FnOnce()>;

static QUEUE: Mutex<Vec<Call>> = Mutex::new(Vec::new());

thread_local! {
    /// Calls the UI thread put off with [`after_frame`]. Never `Send`, so
    /// they stay on the thread that made them.
    static LATER: RefCell<Vec<LocalCall>> = const { RefCell::new(Vec::new()) };
}

/// Queues `call` to run on the UI thread, soon, whether a window is shown
/// or not. It returns once queued.
///
/// The call is `Send`; whatever lives only on the UI thread, like a
/// `TrayIcon`, it reaches through a `thread_local!`.
///
/// ```rust,no_run
/// std::thread::spawn(|| {
///     aurea::on_ui_thread(|| println!("on the UI thread"));
/// });
/// ```
pub fn on_ui_thread(call: impl FnOnce() + Send + 'static) {
    lock(&QUEUE).push(Box::new(call));
    // the pump only turns when there is a frame to make.
    FrameScheduler::wake();
}

/// Runs `call` on the UI thread once the frame being drawn has finished,
/// with nothing borrowed: not the canvas, not the renderer, not the state a
/// draw callback holds.
///
/// For what a frame finds out but cannot do inside one, such as a button
/// drawn on a canvas that opens a [`FileDialog`](crate::dialog::FileDialog),
/// whose own loop draws frames. Unlike [`on_ui_thread`] the call need not be
/// `Send`, so it can carry the app's `Rc` state. Call it on the UI thread;
/// it panics anywhere else, where it could never run.
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
    FrameScheduler::wake();
}

/// Runs everything queued so far, on the UI thread: calls from other
/// threads first, then calls put off with [`after_frame`].
pub(crate) fn drain() {
    let calls = take(&mut *lock(&QUEUE));
    // released first: a call may queue more.
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

    // Both tests exercise drain(), which consumes the process-wide queue.
    static TEST_QUEUE: Mutex<()> = Mutex::new(());

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
