//! Closing one window must not take the whole application with it.
//!
//! The window procedure drops a window from tracking on `WM_DESTROY` and posts
//! the quit message only once nothing is left. Getting that wrong ends the
//! application the moment a popup or tool window closes, which is invisible to
//! every test that uses a single window.
//!
//! `DestroyWindow` dispatches `WM_DESTROY` synchronously, so the decision has
//! already been made by the time it returns and the queue can be inspected
//! directly — no event loop, and no waiting.
//!
//! These create real native windows, so they are `#[ignore]`d. Run them with:
//!
//! ```text
//! cargo test --test win32_multi_window -- --ignored --test-threads=1
//! ```

#![cfg(windows)]

use aurea::{AureaResult, Window};
use std::mem::forget;
use std::os::raw::c_void;
use std::ptr::null_mut;

const WM_QUIT: u32 = 0x0012;
const PM_REMOVE: u32 = 0x0001;

#[repr(C)]
#[derive(Default)]
struct Msg {
    hwnd: *mut c_void,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn DestroyWindow(hwnd: *mut c_void) -> i32;
    fn PeekMessageA(msg: *mut Msg, hwnd: *mut c_void, min: u32, max: u32, remove: u32) -> i32;
}

/// Whether the quit message reaches the queue.
///
/// The quit request is a flag rather than a queued message, and it only
/// surfaces once nothing else is waiting — so a filtered peek walks straight
/// past it while a destroyed window's messages are still pending. Draining
/// the queue is what actually answers the question.
fn quit_was_posted() -> bool {
    let mut msg = Msg {
        hwnd: null_mut(),
        ..Default::default()
    };
    let mut found = false;
    unsafe {
        while PeekMessageA(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                found = true;
            }
        }
    }
    found
}

#[test]
#[ignore = "creates native windows; run with --ignored"]
fn closing_one_window_leaves_the_others_running() -> AureaResult<()> {
    let first = Window::new("first", 200, 150)?;
    let second = Window::new("second", 200, 150)?;
    first.show();
    second.show();

    assert!(!quit_was_posted(), "nothing has closed yet");

    unsafe { DestroyWindow(first.handle()) };
    assert!(
        !quit_was_posted(),
        "closing the first of two windows ended the loop, so the second \
         window's application would have exited with it"
    );

    unsafe { DestroyWindow(second.handle()) };
    assert!(
        quit_was_posted(),
        "closing the last window should end the loop"
    );

    // Both windows are already destroyed. Dropping the Rust values would call
    // DestroyWindow again on stale handles.
    forget(first);
    forget(second);
    Ok(())
}

/// Every window kind used to come out as a plain overlapped window, so asking
/// for a popup or a tool palette silently got something else.
#[test]
#[ignore = "creates native windows; run with --ignored"]
fn each_window_kind_gets_its_own_style() -> AureaResult<()> {
    use aurea::WindowType;

    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const WS_POPUP: u32 = 0x8000_0000;
    const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    const WS_EX_DLGMODALFRAME: u32 = 0x0000_0001;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowLongA(hwnd: *mut c_void, index: i32) -> i32;
    }
    let style = |w: &Window, index| unsafe { GetWindowLongA(w.handle(), index) as u32 };

    let normal = Window::new("normal", 200, 150)?;
    let popup = Window::with_type("popup", 200, 150, WindowType::Popup)?;
    let tool = Window::with_type("tool", 200, 150, WindowType::Tool)?;
    let dialog = Window::with_type("dialog", 200, 150, WindowType::Dialog)?;

    assert_eq!(
        style(&normal, GWL_STYLE) & WS_POPUP,
        0,
        "normal is not a popup"
    );
    assert_ne!(
        style(&popup, GWL_STYLE) & WS_POPUP,
        0,
        "a popup is borderless"
    );
    assert_ne!(
        style(&tool, GWL_EXSTYLE) & WS_EX_TOOLWINDOW,
        0,
        "a tool window stays out of the taskbar"
    );
    assert_ne!(
        style(&dialog, GWL_EXSTYLE) & WS_EX_DLGMODALFRAME,
        0,
        "a dialog has a dialog frame"
    );

    // A sheet is macOS-only, and says so rather than handing back a window
    // that does not behave like one.
    assert!(matches!(
        Window::with_type("sheet", 200, 150, WindowType::Sheet),
        Err(aurea::AureaError::Unsupported { .. })
    ));
    Ok(())
}
