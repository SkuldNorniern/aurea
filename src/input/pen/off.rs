//! Pen events without the `pen` feature: none are built or delivered, and the source stays as it is.

use std::os::raw::c_void;

use aurea_runtime::EventQueue;

use super::PenMode;
use crate::Point;
use crate::window::{PenSample, PenTapAction, WindowEvent};

pub(crate) fn set_mode(_: PenMode) {}

#[allow(clippy::too_many_arguments)]
pub(crate) fn deliver(
    _: *mut c_void,
    _: f64,
    _: f64,
    _: f32,
    _: f32,
    _: f32,
    _: f32,
    _: u32,
    _: u32,
    _: u32,
    _: f64,
) {
}

#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(crate) fn sample(_: PenSample) -> Option<WindowEvent> {
    None
}

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub(crate) fn predicted(_: PenSample) -> Option<WindowEvent> {
    None
}

#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(crate) fn tap(_: bool, _: PenTapAction) -> Option<WindowEvent> {
    None
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn hover(_: Point, _: f64) -> Option<WindowEvent> {
    None
}

pub(crate) fn set_batching(_: &EventQueue, _: bool) {}

pub(crate) fn set_batch_size(_: &EventQueue, _: usize) {}

pub(crate) fn batch_size(_: &EventQueue) -> usize {
    0
}
