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

use log::error;
use std::sync::OnceLock;
use std::thread::{ThreadId, current};

static UI_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Records the calling thread as the UI thread. Called once, from platform
/// initialisation; later calls are ignored.
pub(crate) fn claim() {
    let _ = UI_THREAD.set(current().id());
}

/// Whether the calling thread owns the native UI.
///
/// `true` before the platform has been initialised: there is no UI to get
/// wrong yet.
pub fn is_ui_thread() -> bool {
    match UI_THREAD.get() {
        Some(id) => *id == current().id(),
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

    #[test]
    fn uninitialised_platform_accepts_any_thread() {
        // No `claim()` in this test binary, so nothing is constrained yet.
        assert!(is_ui_thread());
    }
}
