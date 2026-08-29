//! Dropping a window the platform has already destroyed must be harmless.
//!
//! Closing a window from the window manager destroys the widget, but the Rust
//! value that owns it lives until it is dropped, and then asks for the same
//! window to be destroyed again. A GTK widget is a pointer rather than a
//! handle, so the second destroy reads memory that has been freed unless the
//! platform checks first.
//!
//! This lives in its own binary because the UI thread is claimed per process,
//! and a second test in the same binary would run on another thread.
//!
//! ```text
//! cargo test --test gtk_window_drop -- --ignored
//! ```

#![cfg(target_os = "linux")]

use aurea::{AureaResult, Window};
use std::os::raw::c_void;

unsafe extern "C" {
    fn gtk_widget_destroy(widget: *mut c_void);
    fn g_log_set_always_fatal(mask: u32) -> u32;
}

/// GLib reports a destroy of something that is not a widget and carries on, so
/// the bug this covers is a message on stderr rather than a failure. Making
/// that message fatal is what turns it into one.
const G_LOG_LEVEL_CRITICAL: u32 = 1 << 3;

#[test]
#[ignore = "creates a native window; run with --ignored"]
fn dropping_a_window_the_platform_already_closed_is_harmless() -> AureaResult<()> {
    unsafe { g_log_set_always_fatal(G_LOG_LEVEL_CRITICAL) };

    let window = Window::new("already closed", 200, 150)?;
    window.show();

    // What the window manager does when the user clicks the close button.
    unsafe { gtk_widget_destroy(window.handle()) };

    // The owner still believes it has a window to free.
    drop(window);
    Ok(())
}
