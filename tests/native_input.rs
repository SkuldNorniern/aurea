//! A real click has to reach the callback.
//!
//! The unit tests invoke the FFI entry point directly, which proves the
//! registry works and nothing about the path a click actually takes. That path
//! was broken: `WM_COMMAND` carries the control id in `LOWORD(wParam)`, so a
//! button command base of 100000 truncated to 34465, failed the "is this a
//! button" test, and every click was dispatched to the menu registry instead.
//!
//! These need a display and create real native windows, so they are `#[ignore]`d.
//! Run them with:
//!
//! ```text
//! cargo test --test native_input -- --ignored --test-threads=1
//! ```

#![cfg(windows)]

use aurea::elements::{Button, Container, Element, Orientation, Stack};
use aurea::{AureaResult, Window};
use std::cell::Cell;
use std::os::raw::c_void;
use std::rc::Rc;

#[link(name = "user32")]
unsafe extern "system" {
    fn SendMessageA(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
}

/// Tells a button to behave as though it had been clicked, which makes it
/// notify its parent exactly as a real click does.
const BM_CLICK: u32 = 0x00F5;

fn click(hwnd: *mut c_void) {
    unsafe { SendMessageA(hwnd, BM_CLICK, 0, 0) };
}

#[test]
#[ignore = "creates a native window; run with --ignored"]
fn clicking_a_button_runs_its_callback() -> AureaResult<()> {
    let mut window = Window::new("click", 400, 200)?;

    let fired = Rc::new(Cell::new(0));
    let counter = Rc::clone(&fired);
    let button = Button::with_callback("Go", move || counter.set(counter.get() + 1))?;
    let hwnd = button.handle();

    let mut stack = Stack::new(Orientation::Vertical)?;
    stack.add(button)?;
    window.set_content(stack)?;
    window.show();

    click(hwnd);

    assert_eq!(fired.get(), 1, "the click never reached the callback");
    Ok(())
}

/// Buttons sit inside nested containers in real layouts, and the notification
/// has to be forwarded up through each of them.
#[test]
#[ignore = "creates a native window; run with --ignored"]
fn a_click_survives_nested_containers() -> AureaResult<()> {
    let mut window = Window::new("nested click", 400, 200)?;

    let fired = Rc::new(Cell::new(false));
    let flag = Rc::clone(&fired);
    let button = Button::with_callback("Go", move || flag.set(true))?;
    let hwnd = button.handle();

    let mut inner = Stack::new(Orientation::Horizontal)?;
    inner.add(button)?;
    let mut outer = Stack::new(Orientation::Vertical)?;
    outer.add(inner)?;
    window.set_content(outer)?;
    window.show();

    click(hwnd);

    assert!(fired.get(), "the click was lost between the containers");
    Ok(())
}

/// Several buttons must stay distinguishable: their ids share one 16-bit field
/// with menu item ids.
#[test]
#[ignore = "creates a native window; run with --ignored"]
fn each_button_runs_its_own_callback() -> AureaResult<()> {
    let mut window = Window::new("many buttons", 400, 200)?;

    let hits = Rc::new(Cell::new(0usize));
    let mut stack = Stack::new(Orientation::Vertical)?;
    let mut handles = Vec::new();

    for index in 0..5 {
        let counter = Rc::clone(&hits);
        let button = Button::with_callback(&format!("B{index}"), move || {
            // Each button contributes a distinct bit, so a mix-up shows up as
            // the wrong total rather than the right one by luck.
            counter.set(counter.get() | (1 << index));
        })?;
        handles.push(button.handle());
        stack.add(button)?;
    }
    window.set_content(stack)?;
    window.show();

    for hwnd in &handles {
        click(*hwnd);
    }

    assert_eq!(hits.get(), 0b1_1111, "got {:#b}", hits.get());
    Ok(())
}
