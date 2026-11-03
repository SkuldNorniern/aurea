//! Whether a real keyboard is there.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::is_touch;

static CONNECTED: AtomicBool = AtomicBool::new(!is_touch());

/// Whether a hardware keyboard is connected. Tracked on iOS and Android, where
/// [`WindowEvent::KeyboardConnected`](crate::WindowEvent::KeyboardConnected) says when it changes;
/// always true on the desktop.
pub fn keyboard_connected() -> bool {
    CONNECTED.load(Ordering::Acquire)
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
    fn the_desktop_has_a_keyboard() {
        assert!(keyboard_connected());
    }
}
