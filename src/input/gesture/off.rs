//! Gestures without the `gestures` feature: no events.

use std::os::raw::c_void;

pub(crate) fn deliver(_: *mut c_void, _: u32, _: f64, _: f64, _: f64) {}
