//! Whether a real keyboard is there.

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "android")]
use crate::android;
#[cfg(target_os = "ios")]
use crate::ios;
use crate::is_touch;
#[cfg(any(test, target_os = "ios", target_os = "android"))]
use crate::render::Rect;

static CONNECTED: AtomicBool = AtomicBool::new(!is_touch());

/// Whether a hardware keyboard is connected. Tracked on iOS and Android, where
/// [`WindowEvent::KeyboardConnected`](crate::WindowEvent::KeyboardConnected) says when it changes;
/// always true on the desktop.
pub fn keyboard_connected() -> bool {
    CONNECTED.load(Ordering::Acquire)
}

/// How much of the safe area the on-screen keyboard covers from the bottom, in logical pixels:
/// none on the desktop.
pub fn keyboard_height() -> f32 {
    #[cfg(target_os = "ios")]
    return ios::keyboard_height();
    #[cfg(target_os = "android")]
    return android::keyboard_height();
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    0.0
}

static FOLLOWS: AtomicBool = AtomicBool::new(true);

/// Whether the on-screen keyboard rises for a focused text field and falls when focus leaves, on
/// iOS and Android: on by default. Off leaves it to the app's own `show_keyboard`.
pub fn set_keyboard_follows_focus(on: bool) {
    FOLLOWS.store(on, Ordering::Relaxed);
}

/// What a frame should tell the on-screen keyboard, each thing only when it changed.
#[cfg(any(test, target_os = "ios", target_os = "android"))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct KeyboardAsk {
    pub show: Option<bool>,
    pub area: Option<Rect>,
}

/// What the keyboard was last told.
#[cfg(any(test, target_os = "ios", target_os = "android"))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeyboardFocus {
    shown: bool,
    area: Option<Rect>,
}

#[cfg(any(test, target_os = "ios", target_os = "android"))]
impl KeyboardFocus {
    /// What to do after a frame that left `wanted` true when a text field has focus, with its
    /// caret at `caret`.
    pub(crate) fn sync(&mut self, wanted: bool, caret: Option<Rect>) -> KeyboardAsk {
        if !FOLLOWS.load(Ordering::Relaxed) {
            return KeyboardAsk::default();
        }
        let mut ask = KeyboardAsk::default();
        if wanted != self.shown {
            self.shown = wanted;
            ask.show = Some(wanted);
        }
        if !wanted {
            self.area = None;
        } else if caret.is_some() && caret != self.area {
            self.area = caret;
            ask.area = caret;
        }
        ask
    }
}

/// Notes what the host found; true when that is news.
#[cfg(any(target_os = "ios", target_os = "android"))]
pub(crate) fn set_keyboard_connected(connected: bool) -> bool {
    CONNECTED.swap(connected, Ordering::AcqRel) != connected
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod tests {
    use super::*;

    #[test]
    fn it_is_told_only_what_changed() {
        let caret = Rect::new(4.0, 8.0, 2.0, 16.0);
        let moved = Rect::new(4.0, 40.0, 2.0, 16.0);
        let mut focus = KeyboardFocus::default();
        assert_eq!(focus.sync(false, None), KeyboardAsk::default());
        let first = focus.sync(true, Some(caret));
        assert_eq!((first.show, first.area), (Some(true), Some(caret)));
        assert_eq!(focus.sync(true, Some(caret)), KeyboardAsk::default());
        assert_eq!(focus.sync(true, Some(moved)).area, Some(moved));
        assert_eq!(focus.sync(false, Some(moved)).show, Some(false));
        assert_eq!(focus.sync(false, Some(moved)), KeyboardAsk::default());
        let again = focus.sync(true, Some(moved));
        assert_eq!((again.show, again.area), (Some(true), Some(moved)));
    }

    #[test]
    fn the_desktop_has_a_keyboard() {
        assert!(keyboard_connected());
    }
}
