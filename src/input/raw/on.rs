//! Raw mouse deltas.

use std::os::raw::c_void;

use crate::WindowEvent;
use crate::window::push_window_event;

/// Queues a raw mouse delta for `window`.
pub(crate) fn deliver(window: *mut c_void, delta_x: f64, delta_y: f64) {
    push_window_event(window, WindowEvent::RawMouseMotion { delta_x, delta_y });
}
