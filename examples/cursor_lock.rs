use aurea::{CursorGrabMode, KeyCode, Window, WindowEvent};
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::rc::Rc;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let window = Rc::new(Window::new("Cursor Lock Demo", 800, 600)?);
    window.show();

    let locked = Rc::new(Cell::new(false));
    let window_for_events = Rc::clone(&window);
    let locked_for_events = Rc::clone(&locked);
    let last_motion_log = Rc::new(RefCell::new(Instant::now()));
    let last_log_for_events = Rc::clone(&last_motion_log);

    window.on_event(move |event| match event {
        WindowEvent::RawMouseMotion { delta_x, delta_y } => {
            handle_raw_motion(&locked_for_events, &last_log_for_events, delta_x, delta_y);
        }
        WindowEvent::KeyInput { key, pressed, .. } => {
            handle_key_input(&window_for_events, &locked_for_events, key, pressed);
        }
        WindowEvent::MouseButton {
            button, pressed, ..
        } if pressed => {
            println!("mouse pressed: {:?}", button);
            toggle_cursor_lock(&window_for_events, &locked_for_events);
        }
        _ => {}
    });

    window.run()?;
    Ok(())
}

fn handle_raw_motion(locked: &Cell<bool>, last_log: &RefCell<Instant>, delta_x: f64, delta_y: f64) {
    if !locked.get() {
        return;
    }
    if delta_x.abs() < 0.01 && delta_y.abs() < 0.01 {
        return;
    }

    let mut last_log = last_log.borrow_mut();
    if last_log.elapsed() >= Duration::from_millis(250) {
        println!("raw motion: ({:.2}, {:.2})", delta_x, delta_y);
        *last_log = Instant::now();
    }
}

fn handle_key_input(window: &Window, locked: &Cell<bool>, key: KeyCode, pressed: bool) {
    if pressed {
        println!("key pressed: {:?}", key);
    }
    if pressed && key == KeyCode::Escape {
        window.request_close();
    }
    if pressed && key == KeyCode::Space {
        toggle_cursor_lock(window, locked);
    }
}

fn toggle_cursor_lock(window: &Window, locked: &Cell<bool>) {
    locked.set(!locked.get());
    let is_locked = locked.get();
    let mode = if is_locked {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    if window.set_cursor_grab(mode).is_ok() {
        let _ = window.set_cursor_visible(!is_locked);
        let title = if is_locked {
            "Cursor Lock Demo (Locked)"
        } else {
            "Cursor Lock Demo"
        };
        let _ = window.set_title(title);
        println!("cursor locked: {is_locked}");
    }
}
