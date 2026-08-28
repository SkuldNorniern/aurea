//! Reaching the UI thread from somewhere else.

use super::Window;
use crate::registry::handle_key;
use aurea_foundation::lock;
use std::collections::HashMap;
use std::mem::take;
use std::os::raw::c_void;
use std::sync::{LazyLock, Mutex};

type QueuedCall = Box<dyn FnOnce(&Window) + Send>;

static PENDING: LazyLock<Mutex<HashMap<usize, Vec<QueuedCall>>>> =
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
    handle: usize,
}

impl WindowProxy {
    pub(crate) fn new(handle: *mut c_void) -> Self {
        Self {
            handle: handle_key(handle),
        }
    }

    /// Queues `call` to run on the UI thread with the window it came from.
    pub fn dispatch<F>(&self, call: F)
    where
        F: FnOnce(&Window) + Send + 'static,
    {
        lock(&PENDING)
            .entry(self.handle)
            .or_default()
            .push(Box::new(call));
    }

    /// How many calls are queued and not yet run.
    pub fn pending(&self) -> usize {
        lock(&PENDING).get(&self.handle).map_or(0, Vec::len)
    }
}

pub(super) fn drain_for(window: &Window) {
    let calls = {
        let mut pending = lock(&PENDING);
        match pending.get_mut(&handle_key(window.handle())) {
            Some(queue) if !queue.is_empty() => take(queue),
            _ => return,
        }
    };

    for call in calls {
        call(window);
    }
}

pub(super) fn clear_for(handle: *mut c_void) {
    lock(&PENDING).remove(&handle_key(handle));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn proxy_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WindowProxy>();
    }

    #[test]
    fn dispatch_queues_work_without_running_it() {
        let proxy = WindowProxy { handle: 0x1234 };
        let ran = Arc::new(AtomicUsize::new(0));
        let ran_clone = Arc::clone(&ran);

        proxy.dispatch(move |_| {
            ran_clone.fetch_add(1, Ordering::Relaxed);
        });

        assert_eq!(proxy.pending(), 1, "work should be queued");
        assert_eq!(ran.load(Ordering::Relaxed), 0, "and not yet run");
        clear_for(0x1234 as *mut c_void);
    }

    #[test]
    fn clear_discards_queued_work() {
        let proxy = WindowProxy { handle: 0x5678 };
        proxy.dispatch(|_| {});
        assert_eq!(proxy.pending(), 1);

        clear_for(0x5678 as *mut c_void);

        assert_eq!(proxy.pending(), 0);
    }

    #[test]
    fn dispatch_from_another_thread_reaches_the_queue() {
        let proxy = WindowProxy { handle: 0x9ABC };
        let handle = thread::spawn(move || proxy.dispatch(|_| {}));
        handle.join().expect("dispatching thread panicked");

        assert_eq!(proxy.pending(), 1);
        clear_for(0x9ABC as *mut c_void);
    }
}
