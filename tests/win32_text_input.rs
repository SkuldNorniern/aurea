//! Text typed in any script has to arrive intact.
//!
//! The window class used to be registered with `RegisterClassExA`. For an
//! ANSI class Windows delivers `WM_CHAR` carrying a byte in the thread's
//! codepage rather than a UTF-16 code unit, while the text-input path read it
//! as UTF-16 either way — so anything outside that codepage's Latin range
//! arrived as mojibake, and an application wanting Korean, Japanese or
//! Cyrillic input had to guess a codepage and repair the text itself.
//!
//! ```text
//! cargo test --test win32_text_input -- --ignored --test-threads=1
//! ```

#![cfg(windows)]

use aurea::{AureaResult, Window, WindowEvent};
use std::os::raw::c_void;

const WM_CHAR: u32 = 0x0102;

#[link(name = "user32")]
unsafe extern "system" {
    fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
}

/// Types one UTF-16 code unit at the window, the way the keyboard would.
fn type_unit(window: &Window, unit: u16) {
    unsafe { SendMessageW(window.handle(), WM_CHAR, unit as usize, 0) };
}

fn typed_text(window: &Window) -> String {
    window
        .poll_events()
        .into_iter()
        .filter_map(|event| match event {
            WindowEvent::TextInput { text } => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "creates a native window; run with --ignored"]
fn text_outside_the_ansi_codepage_arrives_intact() -> AureaResult<()> {
    let window = Window::new("text", 300, 200)?;
    window.show();
    let _ = window.poll_events();

    // U+D55C HANGUL SYLLABLE HAN, then U+0416 CYRILLIC ZHE: both outside a
    // Latin ANSI codepage, and neither representable in the other's.
    type_unit(&window, 0xD55C);
    type_unit(&window, 0x0416);

    assert_eq!(typed_text(&window), "한Ж");
    Ok(())
}

#[test]
#[ignore = "creates a native window; run with --ignored"]
fn plain_ascii_still_arrives() -> AureaResult<()> {
    let window = Window::new("text", 300, 200)?;
    window.show();
    let _ = window.poll_events();

    for unit in "Hi!".encode_utf16() {
        type_unit(&window, unit);
    }

    assert_eq!(typed_text(&window), "Hi!");
    Ok(())
}

/// Characters outside the BMP arrive as a surrogate pair, two messages.
#[test]
#[ignore = "creates a native window; run with --ignored"]
fn a_surrogate_pair_arrives_as_one_character() -> AureaResult<()> {
    let window = Window::new("text", 300, 200)?;
    window.show();
    let _ = window.poll_events();

    // U+1F600 GRINNING FACE.
    for unit in "\u{1F600}".encode_utf16() {
        type_unit(&window, unit);
    }

    assert_eq!(typed_text(&window), "\u{1F600}");
    Ok(())
}
