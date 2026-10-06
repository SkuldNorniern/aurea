//! A window that closes to the notification area.
//!
//! Closing the window only hides it; the tray icon brings it back with a
//! click, and its menu can quit. A worker thread keeps the tooltip current
//! through `aurea::on_ui_thread`, shown or not. Windows only for now.

use aurea::elements::{Container, Label, Orientation, Stack};
use aurea::{AureaResult, TrayIcon, Window};
use std::cell::RefCell;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

thread_local! {
    /// the icon lives on the UI thread; work sent there finds it here.
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

/// a 32 x 32 round mark, drawn here so the example needs no image file.
fn mark() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32 {
        for x in 0..32 {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let inside = (dx * dx + dy * dy).sqrt() <= 14.0;
            rgba.extend_from_slice(if inside {
                &[110, 178, 255, 255]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    rgba
}

fn main() -> AureaResult<()> {
    let mut window = Window::new("Tray", 420, 200)?;
    let mut content = Stack::new(Orientation::Vertical)?;
    content.add(Label::new(
        "Close this window: it waits in the notification area.",
    )?)?;
    window.set_content(content)?;
    window.set_hide_on_close(true)?;
    window.show();
    let window = Rc::new(window);

    let shown = window.clone();
    let mut tray = TrayIcon::new("Aurea tray example", move || shown.show())?;
    tray.set_icon_rgba(&mark(), 32, 32)?;
    let opened = window.clone();
    tray.add_item("Open", move || opened.show())?;
    tray.add_separator()?;
    let leaving = window.clone();
    tray.add_item("Quit", move || {
        let _ = leaving.set_hide_on_close(false);
        leaving.request_close();
    })?;
    TRAY.with(|held| *held.borrow_mut() = Some(tray));

    thread::spawn(|| {
        for seconds in 1.. {
            thread::sleep(Duration::from_secs(1));
            aurea::on_ui_thread(move || {
                TRAY.with(|held| {
                    if let Some(tray) = &*held.borrow() {
                        let _ = tray.set_tooltip(&format!("Running for {seconds} s"));
                    }
                });
            });
        }
    });

    let result = window.run();
    TRAY.with(|held| held.borrow_mut().take());
    result
}
