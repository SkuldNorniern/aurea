//! A real click has to reach the callback, on GTK.
//!
//! The Windows counterpart in `native_input.rs` exists because the click path
//! was broken there while every unit test passed: those call the FFI entry
//! point directly, which proves the registry works and nothing about the route
//! a click actually takes. GTK carries the button id as object data rather
//! than packed into a message word, so it cannot fail the same way — but the
//! route was untested, which is how the Windows bug survived.
//!
//! These need a display and create real native windows, so they are
//! `#[ignore]`d. Run them with:
//!
//! ```text
//! cargo test --test native_input_gtk -- --ignored --test-threads=1
//! ```

#![cfg(target_os = "linux")]

use aurea::elements::{Button, Container, Element, Orientation, Stack};
use aurea::{AureaResult, Window};
use std::cell::Cell;
use std::os::raw::c_void;
use std::rc::Rc;

unsafe extern "C" {
    fn gtk_button_clicked(button: *mut c_void);
}

/// Emits the button's `clicked` signal, which is what a real press does once
/// GTK has decided a press and release both landed on the widget.
fn click(handle: *mut c_void) {
    unsafe { gtk_button_clicked(handle) };
}

#[test]
#[ignore = "creates a native window; run with --ignored"]
fn clicking_a_button_runs_its_callback() -> AureaResult<()> {
    let mut window = Window::new("click", 400, 200)?;

    let fired = Rc::new(Cell::new(0));
    let counter = Rc::clone(&fired);
    let button = Button::with_callback("Go", move || counter.set(counter.get() + 1))?;
    let handle = button.handle();

    let mut stack = Stack::new(Orientation::Vertical)?;
    stack.add(button)?;
    window.set_content(stack)?;
    window.show();

    click(handle);

    assert_eq!(fired.get(), 1, "the click never reached the callback");
    Ok(())
}

/// Buttons sit inside nested containers in real layouts, and the signal has to
/// survive the reparenting each container does.
#[test]
#[ignore = "creates a native window; run with --ignored"]
fn a_click_survives_nested_containers() -> AureaResult<()> {
    let mut window = Window::new("nested click", 400, 200)?;

    let fired = Rc::new(Cell::new(false));
    let flag = Rc::clone(&fired);
    let button = Button::with_callback("Go", move || flag.set(true))?;
    let handle = button.handle();

    let mut inner = Stack::new(Orientation::Horizontal)?;
    inner.add(button)?;
    let mut outer = Stack::new(Orientation::Vertical)?;
    outer.add(inner)?;
    window.set_content(outer)?;
    window.show();

    click(handle);

    assert!(fired.get(), "the click was lost between the containers");
    Ok(())
}

/// Several buttons must stay distinguishable.
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

    for handle in &handles {
        click(*handle);
    }

    assert_eq!(hits.get(), 0b1_1111, "got {:#b}", hits.get());
    Ok(())
}
