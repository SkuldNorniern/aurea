//! Which thread owns the native UI.

use aurea_foundation::{AureaError, AureaResult, lock};
use log::error;
use std::sync::{LazyLock, Mutex};
use std::thread::{ThreadId, current};

#[cfg(target_vendor = "apple")]
use std::os::raw::c_int;

static UI_THREAD: LazyLock<Mutex<Option<ThreadId>>> = LazyLock::new(|| Mutex::new(None));

pub(crate) fn claim() -> AureaResult<()> {
    if !is_process_main_thread() {
        return Err(AureaError::NotMainThread);
    }
    *lock(&UI_THREAD) = Some(current().id());
    Ok(())
}

#[cfg(not(target_vendor = "apple"))]
fn is_process_main_thread() -> bool {
    true
}

#[cfg(target_vendor = "apple")]
fn is_process_main_thread() -> bool {
    unsafe extern "C" {
        fn pthread_main_np() -> c_int;
    }
    unsafe { pthread_main_np() != 0 }
}

pub(crate) fn release() {
    *lock(&UI_THREAD) = None;
}

/// Whether the calling thread owns the native UI.
pub fn is_ui_thread() -> bool {
    match *lock(&UI_THREAD) {
        Some(id) => id == current().id(),
        None => true,
    }
}

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
    fn the_ui_thread_follows_whoever_claims_it() {
        release();
        assert!(
            is_ui_thread(),
            "nothing is claimed, so nothing is wrong yet"
        );

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
