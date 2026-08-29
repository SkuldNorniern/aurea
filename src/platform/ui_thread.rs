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

use aurea_foundation::{AureaError, AureaResult, lock};
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
pub(crate) fn claim() -> AureaResult<()> {
    if !is_process_main_thread() {
        return Err(AureaError::NotMainThread);
    }
    *lock(&UI_THREAD) = Some(current().id());
    Ok(())
}

/// Whether this is the thread the process started on.
///
/// Only Apple targets care. Everywhere else any single thread may own the UI,
/// so the question does not arise and the answer is always yes.
#[cfg(not(target_vendor = "apple"))]
fn is_process_main_thread() -> bool {
    true
}

/// Whether this is the thread the process started on.
///
/// `pthread_main_np` is in libSystem, which every Apple target links already,
/// so this needs nothing that is not there.
#[cfg(target_vendor = "apple")]
fn is_process_main_thread() -> bool {
    unsafe extern "C" {
        fn pthread_main_np() -> std::os::raw::c_int;
    }
    unsafe { pthread_main_np() != 0 }
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

    /// One test, not two: `UI_THREAD` is process-wide, so separate tests race
    /// each other over it when the suite runs in parallel.
    #[test]
    fn the_ui_thread_follows_whoever_claims_it() {
        release();
        assert!(
            is_ui_thread(),
            "nothing is claimed, so nothing is wrong yet"
        );

        // AppKit and UIKit will not take orders from any thread but the one
        // the process started on, so there the answer is a refusal rather
        // than a handover — including for this test, which libtest runs on a
        // worker thread of its own.
        if cfg!(target_vendor = "apple") {
            assert!(matches!(claim(), Err(AureaError::NotMainThread)));
            release();
            return;
        }

        claim().expect("any thread may own the UI here");
        assert!(is_ui_thread());

        let claimed_elsewhere = thread::spawn(|| {
            release();
            claim().expect("any thread may own the UI here");
            is_ui_thread()
        })
        .join()
        .expect("claiming thread panicked");

        assert!(claimed_elsewhere, "the second thread should own the UI");
        assert!(!is_ui_thread(), "the first thread should have lost it");
        release();
    }
}
