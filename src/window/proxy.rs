//! Reaching the UI thread from somewhere else.
//!
//! [`Window`] is neither `Send` nor `Sync`, because the native window systems
//! underneath it are thread-affine. A background thread that needs to touch the
//! UI therefore cannot hold a window; it holds a [`WindowProxy`], which is a
//! plain handle plus a queue, and asks the UI thread to do the work.
//!
//! Queued work runs the next time the window pumps — `poll_events`,
//! `process_frames`, or the event loop — in the order it was submitted.

use super::{Window, WindowId};
use aurea_foundation::lock;
use aurea_runtime::FrameScheduler;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::mem::take;
use std::sync::{LazyLock, Mutex};

/// Work submitted to a window from another thread.
type QueuedCall = Box<dyn FnOnce(&Window) + Send>;

static PENDING: LazyLock<Mutex<HashMap<WindowId, Vec<QueuedCall>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A `Send + Sync` handle to a window, for use off the UI thread.
///
/// Obtained from [`Window::proxy`]. A proxy does not keep its window alive: if
/// the window is gone, queued work is discarded rather than run against a dead
/// handle.
///
/// # Example
///
/// ```rust,no_run
/// use aurea::Window;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let window = Window::new("App", 800, 600)?;
/// let proxy = window.proxy();
///
/// std::thread::spawn(move || {
///     // Runs on the UI thread, next time the window pumps.
///     proxy.dispatch(|window| {
///         let _ = window.set_title("done");
///     });
/// });
///
/// window.run()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowProxy {
    id: WindowId,
}

impl WindowProxy {
    pub(crate) fn new(id: WindowId) -> Self {
        Self { id }
    }

    /// Queues `call` to run on the UI thread with the window it came from.
    ///
    /// Returns once the work is queued, not once it has run. Work queued for a
    /// window that is never pumped again is never run.
    pub fn dispatch<F>(&self, call: F) -> Result<(), WindowClosed>
    where
        F: FnOnce(&Window) + Send + 'static,
    {
        {
            let mut pending = lock(&PENDING);
            // A queue exists for exactly as long as its window does. Adding
            // one back for a window that has gone would keep the call, and
            // everything it captured, for the life of the process: nothing
            // drains a queue no window is reading.
            let Some(queue) = pending.get_mut(&self.id) else {
                return Err(WindowClosed);
            };
            queue.push(Box::new(call));
        }

        // Queuing is not enough: an idle UI would sit there until something
        // else happened to pump. Ask for a frame so the work is picked up.
        FrameScheduler::schedule();
        Ok(())
    }

    /// How many calls are queued and not yet run.
    pub fn pending(&self) -> usize {
        lock(&PENDING).get(&self.id).map_or(0, Vec::len)
    }
}

/// Runs everything queued for `window`. Called from the window's own pumping
/// methods, on the UI thread.
pub(super) fn drain_for(window: &Window) {
    let calls = {
        let mut pending = lock(&PENDING);
        match pending.get_mut(&window.proxy_id()) {
            Some(queue) if !queue.is_empty() => take(queue),
            _ => return,
        }
    };

    // The lock is released first: a queued call may queue more work, or create
    // and destroy windows.
    for call in calls {
        call(window);
    }
}

/// Discards work queued for a window that is going away.
///
/// Proxies that outlive the window keep working in the sense that they accept
/// calls; those calls are simply never run, because the id is never reused.
pub(super) fn clear_for(id: WindowId) {
    lock(&PENDING).remove(&id);
}

/// Opens a queue for a new window, which is what makes its proxies usable.
pub(super) fn register(id: WindowId) {
    lock(&PENDING).entry(id).or_default();
}

/// Returned by [`WindowProxy::dispatch`] when the window is gone.
///
/// The work was not queued and will not run. A proxy does not keep its window
/// alive, so this is an ordinary outcome rather than a fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowClosed;

impl Display for WindowClosed {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("the window this proxy refers to has closed")
    }
}

impl Error for WindowClosed {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::raw::c_void;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    /// A proxy for a window that exists, which is what `Window::new` sets up.
    ///
    /// The handle is a stand-in: nothing here dereferences it, and each call
    /// uses a fresh one so the ids differ the way real windows' do.
    fn live_proxy() -> WindowProxy {
        let id = fresh_id();
        register(id);
        WindowProxy::new(id)
    }

    /// An id from a handle no other test is using.
    fn fresh_id() -> WindowId {
        static NEXT: AtomicUsize = AtomicUsize::new(0x5000);
        let handle = NEXT.fetch_add(0x10, Ordering::Relaxed) as *mut c_void;
        WindowId::claim(handle)
    }

    /// A proxy outliving its window used to put its queue back and hold the
    /// call, and everything it captured, for the life of the process.
    #[test]
    fn dispatch_after_the_window_closed_is_refused_and_keeps_nothing() {
        let proxy = live_proxy();
        clear_for(proxy.id);

        let outcome = proxy.dispatch(|_| {});

        assert_eq!(outcome, Err(WindowClosed));
        assert_eq!(proxy.pending(), 0, "a closed window must retain no work");
    }

    #[test]
    fn proxy_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WindowProxy>();
    }

    #[test]
    fn dispatch_queues_work_without_running_it() {
        let proxy = live_proxy();
        let ran = Arc::new(AtomicUsize::new(0));
        let ran_clone = Arc::clone(&ran);

        proxy
            .dispatch(move |_| {
                ran_clone.fetch_add(1, Ordering::Relaxed);
            })
            .expect("the window is live");

        assert_eq!(proxy.pending(), 1, "work should be queued");
        assert_eq!(ran.load(Ordering::Relaxed), 0, "and not yet run");
        clear_for(proxy.id);
    }

    #[test]
    fn clear_discards_queued_work() {
        let proxy = live_proxy();
        proxy.dispatch(|_| {}).expect("the window is live");
        assert_eq!(proxy.pending(), 1);

        clear_for(proxy.id);

        assert_eq!(proxy.pending(), 0);
    }

    #[test]
    fn dispatch_from_another_thread_reaches_the_queue() {
        let proxy = live_proxy();
        let id = proxy.id;
        let handle = thread::spawn(move || proxy.dispatch(|_| {}));
        handle
            .join()
            .expect("dispatching thread panicked")
            .expect("the window is live");

        assert_eq!(WindowProxy::new(id).pending(), 1);
        clear_for(id);
    }

    /// Ids are never reused, so a proxy that outlives its window cannot be
    /// pointed at whichever window takes over its native handle.
    #[test]
    fn ids_are_not_reused() {
        let first = fresh_id();
        let second = fresh_id();
        assert_ne!(first, second);

        register(second);
        let stale = WindowProxy::new(first);
        clear_for(first);

        // Refused outright, and in particular not delivered to whichever
        // window took over the native handle the first one had.
        assert_eq!(stale.dispatch(|_| {}), Err(WindowClosed));
        assert_eq!(WindowProxy::new(second).pending(), 0);
        clear_for(second);
    }
}
