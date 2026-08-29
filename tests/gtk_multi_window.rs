//! Closing one window must not take the whole application with it.
//!
//! `ng_linux_run` enters `gtk_main`, and the window `destroy` handler used to
//! call `gtk_main_quit` unconditionally — so on GTK, closing any one window of
//! several ended the loop and every remaining window went with it. The
//! `poll_events` tests cannot catch that: they drive the main context directly
//! and never enter `gtk_main`, which is the only state in which the bug shows.
//!
//! This needs a display and runs a real main loop, so it is `#[ignore]`d. Run
//! it with:
//!
//! ```text
//! cargo test --test gtk_multi_window -- --ignored
//! ```

#![cfg(target_os = "linux")]

use aurea::{AureaResult, Window};
use std::os::raw::{c_int, c_void};
use std::sync::atomic::{AtomicI32, Ordering};

unsafe extern "C" {
    fn gtk_widget_destroy(widget: *mut c_void);
    fn gtk_main_level() -> c_uint;
    fn g_idle_add(function: extern "C" fn(*mut c_void) -> c_int, data: *mut c_void) -> c_uint;
    fn g_timeout_add(
        interval: c_uint,
        function: extern "C" fn(*mut c_void) -> c_int,
        data: *mut c_void,
    ) -> c_uint;
}
#[allow(non_camel_case_types)]
type c_uint = u32;

const REMOVE: c_int = 0;

/// The loop depth seen after the first window closed. Zero means the loop had
/// already been quit, which is the bug.
static LEVEL_AFTER_FIRST_CLOSE: AtomicI32 = AtomicI32::new(-1);
/// The second window, closed once the check has run so the loop can end.
static SECOND: AtomicI32 = AtomicI32::new(0);
static SECOND_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static FIRST_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

extern "C" fn close_first(_: *mut c_void) -> c_int {
    unsafe { gtk_widget_destroy(FIRST_PTR.load(Ordering::SeqCst) as *mut c_void) };
    REMOVE
}

/// Runs after the first window is gone. If the loop is still turning we get
/// here at all, and the depth it reports is the evidence.
extern "C" fn observe_then_close_second(_: *mut c_void) -> c_int {
    LEVEL_AFTER_FIRST_CLOSE.store(unsafe { gtk_main_level() } as i32, Ordering::SeqCst);
    unsafe { gtk_widget_destroy(SECOND_PTR.load(Ordering::SeqCst) as *mut c_void) };
    REMOVE
}

/// Stops a hung loop so a failure is a failed assert rather than a hang.
extern "C" fn bail_out(_: *mut c_void) -> c_int {
    unsafe {
        if gtk_main_level() > 0 {
            gtk_main_quit();
        }
    }
    REMOVE
}

unsafe extern "C" {
    fn gtk_main_quit();
}

#[test]
#[ignore = "runs a real GTK main loop; run with --ignored"]
fn closing_one_window_leaves_the_others_running() -> AureaResult<()> {
    let first = Window::new("first", 200, 150)?;
    let second = Window::new("second", 200, 150)?;
    first.show();
    second.show();

    FIRST_PTR.store(first.handle() as usize, Ordering::SeqCst);
    SECOND_PTR.store(second.handle() as usize, Ordering::SeqCst);
    SECOND.store(1, Ordering::SeqCst);

    unsafe {
        g_idle_add(close_first, std::ptr::null_mut());
        g_timeout_add(150, observe_then_close_second, std::ptr::null_mut());
        g_timeout_add(3000, bail_out, std::ptr::null_mut());
    }

    // Returns when the last window goes, or when bail_out fires.
    first.run()?;

    // Both widgets are already gone, destroyed above. Dropping the Rust values
    // would destroy them a second time, which GTK reports and ignores; leaking
    // them keeps that noise out of the test output.
    std::mem::forget(first);
    std::mem::forget(second);

    assert!(
        LEVEL_AFTER_FIRST_CLOSE.load(Ordering::SeqCst) > 0,
        "the loop ended when the first of two windows closed, so the second \
         window's application would have exited with it"
    );
    Ok(())
}
