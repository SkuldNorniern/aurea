//! Pads on Windows, Linux, macOS and iOS: the platform source reads each pad's state, and [`Hub`] turns the
//! changes into events.

use std::collections::BTreeSet;
use std::os::raw::c_void;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard, PoisonError};

use super::PadEvent;
use super::PadInfo;
use super::state::{Found, Hub, gate};
use crate::ffi::ng_platform_window_is_focused;
use crate::registry::window::register_event_callback;
use crate::window::WindowEvent;

#[cfg(target_vendor = "apple")]
use super::apple::Source;
#[cfg(target_os = "linux")]
use super::evdev::Source;
#[cfg(windows)]
use super::win::Source;

/// Handles of the windows that have focus.
static FOCUSED: Mutex<BTreeSet<usize>> = Mutex::new(BTreeSet::new());

fn focused_set() -> MutexGuard<'static, BTreeSet<usize>> {
    FOCUSED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether one of the app's windows has focus, from the focus events they have dispatched.
pub(crate) fn is_focused() -> bool {
    !focused_set().is_empty()
}

/// Follows the window's focus events from now on, and takes its focus as it is now.
pub(crate) fn watch_window(handle: *mut c_void) {
    listen(handle);
    // SAFETY: the handle is a live window; the caller has just created it.
    if unsafe { ng_platform_window_is_focused(handle) } != 0 {
        focused_set().insert(handle as usize);
    }
}

/// Drops the window from the focus set, as the window goes away.
pub(crate) fn forget_window(handle: *mut c_void) {
    focused_set().remove(&(handle as usize));
}

fn listen(handle: *mut c_void) {
    register_event_callback(
        handle,
        Rc::new(move |event: &WindowEvent| match event {
            WindowEvent::Focused => {
                focused_set().insert(handle as usize);
            }
            WindowEvent::Unfocused => {
                focused_set().remove(&(handle as usize));
            }
            _ => {}
        }),
    );
}

/// Game controllers, polled by the app on its event loop thread.
///
/// Pads deliver only while one of the app's windows has focus. On focus loss each held button
/// is released and each moved axis zeroed; on regain the pads' state is sent from released.
/// Connections and disconnections come at any time.
///
/// ```rust,no_run
/// use aurea::input::gamepad::{PadEvent, Pads};
///
/// let mut pads = Pads::new();
/// let mut events = Vec::new();
/// pads.poll(&mut events);
/// for event in events.drain(..) {
///     if let PadEvent::Button { button, down: true, .. } = event {
///         println!("{button:?}");
///     }
/// }
/// ```
pub struct Pads {
    hub: Hub,
    source: Source,
    found: Vec<Found>,
}

impl Pads {
    /// Starts looking for pads. Nothing is reported until [`Pads::poll`].
    pub fn new() -> Pads {
        Pads {
            hub: Hub::default(),
            source: Source::new(),
            found: Vec::new(),
        }
    }

    /// Reads every pad and appends the events since the last poll to `events`, connections
    /// included. Call it at the rate the app reads input, such as once a frame.
    pub fn poll(&mut self, events: &mut Vec<PadEvent>) {
        self.found.clear();
        self.source.read(&mut self.found);
        gate(&mut self.found, is_focused());
        self.hub.update(&self.found, events);
    }

    /// The pads connected now, in the order they came.
    pub fn list(&self) -> &[PadInfo] {
        self.hub.list()
    }
}

impl Default for Pads {
    fn default() -> Pads {
        Pads::new()
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::*;
    use crate::registry::window::{dispatch_window_events, unregister_event_callbacks};

    fn focused_by(handle: *mut c_void) -> bool {
        focused_set().contains(&(handle as usize))
    }

    #[test]
    fn focus_signal_follows_window_focus() {
        let a = 0x6a01 as *mut c_void;
        let b = 0x6a02 as *mut c_void;
        listen(a);
        listen(b);

        dispatch_window_events(a, &[WindowEvent::Focused]);
        assert!(focused_by(a) && is_focused());

        dispatch_window_events(b, &[WindowEvent::Focused]);
        dispatch_window_events(a, &[WindowEvent::Unfocused]);
        assert!(!focused_by(a) && focused_by(b));
        assert!(is_focused(), "focus moved to another window of the app");

        dispatch_window_events(b, &[WindowEvent::Unfocused]);
        assert!(!focused_by(b));

        dispatch_window_events(a, &[WindowEvent::Focused]);
        forget_window(a);
        assert!(!focused_by(a), "a closed window is not focused");

        unregister_event_callbacks(a);
        unregister_event_callbacks(b);
    }
}
