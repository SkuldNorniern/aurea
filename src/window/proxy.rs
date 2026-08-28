//! Reaching the UI thread from somewhere else.
//!
//! [`Window`] is neither `Send` nor `Sync`, because the native window systems
//! underneath it are thread-affine. A background thread that needs to touch the
//! UI therefore cannot hold a window; it holds a [`WindowProxy`], which is a
//! plain handle plus a queue, and asks the UI thread to do the work.
//!
//! Queued work runs the next time the window pumps — `poll_events`,
//! `process_frames`, or the event loop — in the order it was submitted.

use super::Window;
use aurea_foundation::lock;
use aurea_runtime::FrameScheduler;
use std::collections::HashMap;
use std::mem::take;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

/// Work submitted to a window from another thread.
type QueuedCall = Box<dyn FnOnce(&Window) + Send>;

/// Identifies one window for the life of the process.
///
/// Not the native handle: the platform reuses addresses, so a proxy held past
/// its window's death could otherwise deliver work to whichever window landed
/// on the same address next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProxyId(u64);

static NEXT_PROXY_ID: AtomicU64 = AtomicU64::new(1);

/// Allocates an id for a new window.
pub(super) fn next_id() -> ProxyId {
    ProxyId(NEXT_PROXY_ID.fetch_add(1, Ordering::Relaxed))
}

static PENDING: LazyLock<Mutex<HashMap<ProxyId, Vec<QueuedCall>>>> =
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
    id: ProxyId,
}

impl WindowProxy {
    pub(crate) fn new(id: ProxyId) -> Self {
        Self { id }
    }

    /// Queues `call` to run on the UI thread with the window it came from.
    ///
    /// Returns once the work is queued, not once it has run. Work queued for a
    /// window that is never pumped again is never run.
    pub fn dispatch<F>(&self, call: F)
    where
        F: FnOnce(&Window) + Send + 'static,
    {
        lock(&PENDING)
            .entry(self.id)
            .or_default()
            .push(Box::new(call));

        // Queuing is not enough: an idle UI would sit there until something
        // else happened to pump. Ask for a frame so the work is picked up.
        FrameScheduler::schedule();
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
pub(super) fn clear_for(id: ProxyId) {
    lock(&PENDING).remove(&id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::thread;

    #[test]
    fn proxy_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WindowProxy>();
    }

    #[test]
    fn dispatch_queues_work_without_running_it() {
        let proxy = WindowProxy::new(next_id());
        let ran = Arc::new(AtomicUsize::new(0));
        let ran_clone = Arc::clone(&ran);

        proxy.dispatch(move |_| {
            ran_clone.fetch_add(1, Ordering::Relaxed);
        });

        assert_eq!(proxy.pending(), 1, "work should be queued");
        assert_eq!(ran.load(Ordering::Relaxed), 0, "and not yet run");
        clear_for(proxy.id);
    }

    #[test]
    fn clear_discards_queued_work() {
        let proxy = WindowProxy::new(next_id());
        proxy.dispatch(|_| {});
        assert_eq!(proxy.pending(), 1);

        clear_for(proxy.id);

        assert_eq!(proxy.pending(), 0);
    }

    #[test]
    fn dispatch_from_another_thread_reaches_the_queue() {
        let proxy = WindowProxy::new(next_id());
        let handle = thread::spawn(move || proxy.dispatch(|_| {}));
        handle.join().expect("dispatching thread panicked");

        assert_eq!(proxy.pending(), 1);
        clear_for(proxy.id);
    }

    /// Ids are never reused, so a proxy that outlives its window cannot be
    /// pointed at whichever window takes over its native handle.
    #[test]
    fn ids_are_not_reused() {
        let first = next_id();
        let second = next_id();
        assert_ne!(first, second);

        let stale = WindowProxy::new(first);
        clear_for(first);
        stale.dispatch(|_| {});

        // The work sits under the dead id and never reaches the new window.
        assert_eq!(WindowProxy::new(second).pending(), 0);
        clear_for(first);
    }
}
