//! Which thread owns the native UI.

use aurea_foundation::lock;
use log::error;
use std::sync::{LazyLock, Mutex};
use std::thread::{ThreadId, current};

static UI_THREAD: LazyLock<Mutex<Option<ThreadId>>> = LazyLock::new(|| Mutex::new(None));

pub(crate) fn claim() {
    *lock(&UI_THREAD) = Some(current().id());
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

        claim();
        assert!(is_ui_thread());

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
