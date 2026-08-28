//! Which thread owns the native UI.

use log::error;
use std::sync::OnceLock;
use std::thread::{ThreadId, current};

static UI_THREAD: OnceLock<ThreadId> = OnceLock::new();

pub(crate) fn claim() {
    let _ = UI_THREAD.set(current().id());
}

/// Whether the calling thread owns the native UI.
pub fn is_ui_thread() -> bool {
    match UI_THREAD.get() {
        Some(id) => *id == current().id(),
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

    #[test]
    fn uninitialised_platform_accepts_any_thread() {
        assert!(is_ui_thread());
    }
}
