//! Reaching the UI thread from somewhere else.

use super::Window;
use aurea_foundation::lock;
use aurea_runtime::FrameScheduler;
use std::collections::HashMap;
use std::mem::take;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

type QueuedCall = Box<dyn FnOnce(&Window) + Send>;

/// Identifies one window for the life of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProxyId(u64);

static NEXT_PROXY_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_id() -> ProxyId {
    ProxyId(NEXT_PROXY_ID.fetch_add(1, Ordering::Relaxed))
}

static PENDING: LazyLock<Mutex<HashMap<ProxyId, Vec<QueuedCall>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A `Send + Sync` handle to a window, for use off the UI thread.
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
    pub fn dispatch<F>(&self, call: F)
    where
        F: FnOnce(&Window) + Send + 'static,
    {
        lock(&PENDING)
            .entry(self.id)
            .or_default()
            .push(Box::new(call));

        FrameScheduler::schedule();
    }

    /// How many calls are queued and not yet run.
    pub fn pending(&self) -> usize {
        lock(&PENDING).get(&self.id).map_or(0, Vec::len)
    }
}

pub(super) fn drain_for(window: &Window) {
    let calls = {
        let mut pending = lock(&PENDING);
        match pending.get_mut(&window.proxy_id()) {
            Some(queue) if !queue.is_empty() => take(queue),
            _ => return,
        }
    };

    for call in calls {
        call(window);
    }
}

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

    #[test]
    fn ids_are_not_reused() {
        let first = next_id();
        let second = next_id();
        assert_ne!(first, second);

        let stale = WindowProxy::new(first);
        clear_for(first);
        stale.dispatch(|_| {});

        assert_eq!(WindowProxy::new(second).pending(), 0);
        clear_for(first);
    }
}
