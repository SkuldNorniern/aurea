//! The mouse events a finger or pencil sends, as a desktop mouse would send them.

use std::time::Duration;

use crate::{Modifiers, MouseButton, MouseButtons, Point, PointerKind, WindowEvent};

pub(super) fn long_press(at: Point) -> WindowEvent {
    WindowEvent::LongPress {
        x: f64::from(at.x),
        y: f64::from(at.y),
    }
}

pub(super) fn press(out: &mut Vec<WindowEvent>, at: Point) {
    out.push(mouse_move(at, false));
    out.push(button(at, true));
}

pub(super) fn release(out: &mut Vec<WindowEvent>, at: Point) {
    out.push(button(at, false));
    out.push(WindowEvent::MouseExited);
}

/// Marks the mouse events in `events` as a pencil's, which [`mouse_move`] and [`button`] make as a
/// finger's.
pub(super) fn as_pen(events: &mut [WindowEvent]) {
    for event in events {
        if let WindowEvent::MouseMove { pointer, .. } | WindowEvent::MouseButton { pointer, .. } =
            event
        {
            *pointer = PointerKind::Pen;
        }
    }
}

pub(super) fn mouse_move(at: Point, held: bool) -> WindowEvent {
    let buttons = if held {
        MouseButtons::from_bits(1 << MouseButton::Left.to_raw())
    } else {
        MouseButtons::default()
    };
    WindowEvent::MouseMove {
        x: f64::from(at.x),
        y: f64::from(at.y),
        buttons,
        modifiers: Modifiers::default(),
        time: Duration::default(),
        pointer: PointerKind::Touch,
    }
}

fn button(at: Point, pressed: bool) -> WindowEvent {
    WindowEvent::MouseButton {
        button: MouseButton::Left,
        pressed,
        modifiers: Modifiers::default(),
        x: f64::from(at.x),
        y: f64::from(at.y),
        click_count: 1,
        time: Duration::default(),
        pointer: PointerKind::Touch,
    }
}
