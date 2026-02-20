use aurea::{CursorGrabMode, KeyCode, Window, WindowEvent};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = Arc::new(Window::new("Cursor Lock Demo", 800, 600)?);
    window.show();

    let locked = Arc::new(Mutex::new(false));
    let window_for_events = Arc::clone(&window);
    let locked_for_events = Arc::clone(&locked);
    let last_motion_log = Arc::new(Mutex::new(Instant::now()));
    let last_log_for_events = Arc::clone(&last_motion_log);

    window.on_event(move |event| match event {
        WindowEvent::RawMouseMotion { delta_x, delta_y } => {
            let locked = *locked_for_events.lock().unwrap();
            if !locked {
                return;
            }
            if delta_x.abs() < 0.01 && delta_y.abs() < 0.01 {
                return;
            }

            let mut last_log = last_log_for_events.lock().unwrap();
            if last_log.elapsed() >= Duration::from_millis(250) {
                println!("raw motion: ({:.2}, {:.2})", delta_x, delta_y);
                *last_log = Instant::now();
            }
        }
        WindowEvent::KeyInput { key, pressed, .. } => {
            if pressed {
                println!("key pressed: {:?}", key);
            }
            if pressed && key == KeyCode::Escape {
                window_for_events.request_close();
            }
            if pressed && key == KeyCode::Space {
                let mut locked = locked_for_events.lock().unwrap();
                *locked = !*locked;
                let mode = if *locked {
                    CursorGrabMode::Locked
                } else {
                    CursorGrabMode::None
                };
                if window_for_events.set_cursor_grab(mode).is_ok() {
                    let _ = window_for_events.set_cursor_visible(!*locked);
                    let title = if *locked {
                        "Cursor Lock Demo (Locked)"
                    } else {
                        "Cursor Lock Demo"
                    };
                    let _ = window_for_events.set_title(title);
                    println!("cursor locked: {}", *locked);
                }
            }
        }
        WindowEvent::MouseButton {
            button, pressed, ..
        } => {
            if pressed {
                println!("mouse pressed: {:?}", button);
                let mut locked = locked_for_events.lock().unwrap();
                *locked = !*locked;
                let mode = if *locked {
                    CursorGrabMode::Locked
                } else {
                    CursorGrabMode::None
                };
                if window_for_events.set_cursor_grab(mode).is_ok() {
                    let _ = window_for_events.set_cursor_visible(!*locked);
                    let title = if *locked {
                        "Cursor Lock Demo (Locked)"
                    } else {
                        "Cursor Lock Demo"
                    };
                    let _ = window_for_events.set_title(title);
                    println!("cursor locked: {}", *locked);
                }
            }
        }
        _ => {}
    });

    window.run()?;
    Ok(())
}
