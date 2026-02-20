//! Input smoke test: key, mouse, wheel, focus, cursor.
//!
//! Minimal window that prints every input event to verify the pipeline.
//! Run and interact (keys, mouse, scroll, tab away/back); Escape closes.

use aurea::{KeyCode, Window, WindowEvent};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = Arc::new(Window::new("Input Smoke", 640, 480)?);
    window.show();

    let w = Arc::clone(&window);
    window.on_event(move |event| {
        match event {
            WindowEvent::KeyInput { key, pressed, modifiers } => {
                let action = if pressed { "key down" } else { "key up" };
                let mods = if modifiers.is_any() {
                    format!(
                        " [shift={} ctrl={} alt={} meta={}]",
                        modifiers.shift, modifiers.ctrl, modifiers.alt, modifiers.meta
                    )
                } else {
                    String::new()
                };
                println!("{} {:?}{}", action, key, mods);
                if pressed && key == KeyCode::Escape {
                    w.request_close();
                }
            }
            WindowEvent::MouseButton { button, pressed, .. } => {
                let action = if pressed { "mouse down" } else { "mouse up" };
                println!("{} {:?}", action, button);
            }
            WindowEvent::MouseMove { x, y } => {
                println!("mouse move ({:.1}, {:.1})", x, y);
            }
            WindowEvent::MouseWheel { delta_x, delta_y, .. } => {
                println!("wheel ({:.1}, {:.1})", delta_x, delta_y);
            }
            WindowEvent::TextInput { text } => {
                println!("text input: {:?}", text);
            }
            WindowEvent::Focused => println!("focus gained"),
            WindowEvent::Unfocused => println!("focus lost"),
            WindowEvent::MouseEntered => println!("cursor entered"),
            WindowEvent::MouseExited => println!("cursor left"),
            _ => {}
        }
    });

    window.run()?;
    Ok(())
}
