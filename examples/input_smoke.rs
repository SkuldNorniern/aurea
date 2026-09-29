//! Input smoke test: key, mouse, wheel, focus, cursor.
//!
//! Window with a canvas that prints every input event to verify the pipeline.
//! Most apps get their input over a canvas, so moving over it must not look
//! like leaving the window. Run and interact (keys, mouse, scroll, tab
//! away/back, drag out of the window); Escape closes.

use aurea::elements::{Orientation, Stack};
use aurea::render::{Canvas, Color, RendererBackend};
use aurea::{Container, CursorIcon, KeyCode, Window, WindowEvent};
use std::error::Error;
use std::rc::Rc;

fn main() -> Result<(), Box<dyn Error>> {
    let mut window = Window::new("Input Smoke", 640, 480)?;
    let canvas = Canvas::new(640, 480, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(240, 240, 240));
    let mut content = Stack::new(Orientation::Vertical)?;
    content.add(canvas)?;
    window.set_content(content)?;

    let window = Rc::new(window);
    window.show();

    let w = Rc::clone(&window);
    window.on_event(move |event| match event {
        WindowEvent::KeyInput {
            key,
            pressed,
            modifiers,
        } => {
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
        WindowEvent::MouseButton {
            button,
            pressed,
            x,
            y,
            click_count,
            ..
        } => {
            let action = if pressed { "mouse down" } else { "mouse up" };
            println!("{action} {button:?} ({x:.1}, {y:.1}) x{click_count}");
        }
        WindowEvent::MouseMove { x, y, buttons, .. } => {
            // The right half shows a hand, to check cursor shapes.
            let _ = w.set_cursor(if x > 320.0 {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            });
            if buttons.is_empty() {
                println!("mouse move ({x:.1}, {y:.1})");
            } else {
                println!("mouse drag ({x:.1}, {y:.1}) buttons {:#b}", buttons.bits());
            }
        }
        WindowEvent::MouseWheel {
            delta_x, delta_y, ..
        } => {
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
    });

    window.run()?;
    Ok(())
}
