//! Prints game controller events until the window is closed.
//!
//! Opens a small window: pads report only while one of the app's windows has focus, and the
//! window's focus arrives through its events, which `poll_events` delivers each pass.

use std::thread::sleep;
use std::time::Duration;

use aurea::input::gamepad::Pads;
use aurea::{AureaResult, Window, WindowEvent};

fn main() -> AureaResult<()> {
    let window = Window::new("Gamepad", 320, 200)?;
    let mut pads = Pads::new();
    let mut events = Vec::new();
    loop {
        let closing = window
            .poll_events()
            .iter()
            .any(|event| matches!(event, WindowEvent::CloseRequested));
        pads.poll(&mut events);
        for event in events.drain(..) {
            println!("{event:?}");
        }
        if closing {
            break;
        }
        sleep(Duration::from_millis(4));
    }
    Ok(())
}
