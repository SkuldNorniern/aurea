//! Work handed to the UI thread from any other thread, without a window.
//!
//! [`WindowProxy::dispatch`](crate::window::WindowProxy::dispatch) needs the
//! window, and only runs when that window is pumped by hand. Things that are
//! not windows, like a [`TrayIcon`](crate::TrayIcon), and apps that sit in
//! [`Window::run`](crate::Window::run), use this instead: the platform's own
//! pump runs the queue.

use std::mem::take;
use std::sync::Mutex;

use aurea_foundation::lock;
use aurea_runtime::FrameScheduler;

type Call = Box<dyn FnOnce() + Send>;

static QUEUE: Mutex<Vec<Call>> = Mutex::new(Vec::new());

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

/// Runs everything queued so far, on the UI thread.
pub(crate) fn drain() {
    let calls = take(&mut *lock(&QUEUE));
    // released first: a call may queue more.
    for call in calls {
        call();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn queued_calls_run_once_in_order_and_may_queue_more() {
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
}
