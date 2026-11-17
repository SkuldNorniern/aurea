//! Raw mouse deltas without the `raw-mouse` feature: none are delivered.

use std::os::raw::c_void;

pub(crate) fn deliver(_: *mut c_void, _: f64, _: f64) {}
