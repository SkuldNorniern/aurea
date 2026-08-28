//! Which thread owns the native UI.
//!
//! Native window systems are thread-affine: AppKit and UIKit require their
//! objects to be touched from the main thread, GTK from the thread that called
//! `gtk_init`, and Win32 delivers messages only to the thread that created the
//! window. Aurea inherits that constraint, but nothing in the type system
//! expresses it — [`Window`](crate::Window) is `Send + Sync` so that event
//! callbacks, which run on the UI thread, can capture an `Arc<Window>`.
//!
//! This module records the thread that initialised the platform so the
//! constraint is at least *checked*: a debug build reports the violation at
//! the offending call, and a release build logs it, instead of the native
//! toolkit misbehaving somewhere far away.

use aurea_foundation::lock;
use log::error;
use std::sync::{LazyLock, Mutex};
use std::thread::{ThreadId, current};

/// The thread that last brought the platform up, if it is up.
static UI_THREAD: LazyLock<Mutex<Option<ThreadId>>> = LazyLock::new(|| Mutex::new(None));

/// Records the calling thread as the UI thread.
///
/// Called from platform initialisation, which can run more than once: the
/// platform is torn down with the last window and brought back up with the
/// next one, possibly on a different thread.
pub(crate) fn claim() {
    *lock(&UI_THREAD) = Some(current().id());
}

/// Forgets the UI thread. Called when the platform is torn down.
pub(crate) fn release() {
    *lock(&UI_THREAD) = None;
}

/// Whether the calling thread owns the native UI.
///
/// `true` while the platform is down: there is no UI to get wrong yet.
pub fn is_ui_thread() -> bool {
    match *lock(&UI_THREAD) {
        Some(id) => id == current().id(),
        None => true,
    }
}

/// Reports a native call made from the wrong thread.
///
/// `operation` names the call site, so the message points at the offending
/// operation rather than at this module.
pub(crate) fn check(operation: &str) {
    if is_ui_thread() {
        return;
    }
    error!(
        "aurea: {operation} was called from a non-UI thread; native window \
         systems require UI calls on the thread that initialised the platform"
    );
    debug_assert!(
        false,
        "{operation} called off the UI thread — see aurea::platform::ui_thread"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn uninitialised_platform_accepts_any_thread() {
        release();
        assert!(is_ui_thread());
    }

    #[test]
    fn a_released_platform_can_be_claimed_by_another_thread() {
        claim();
        let claimed_elsewhere = thread::spawn(|| {
            release();
            claim();
            is_ui_thread()
        })
        .join()
        .expect("claiming thread panicked");

        assert!(claimed_elsewhere, "the second thread should own the UI");
        assert!(!is_ui_thread(), "the first thread should have lost it");
        release();
    }
}
