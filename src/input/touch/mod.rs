//! Turns touches on a phone or tablet screen into the mouse, wheel and pinch events a desktop
//! window sends, so the same UI code runs on both.
//!
//! A pencil is the mouse at once. A finger waits a moment first: it becomes the mouse when it moves
//! or stays down, a click when it lifts, and the start of a two-finger gesture when a second finger
//! lands. Two fingers moving together scroll as [`WindowEvent::MouseWheel`], spreading or closing
//! zoom as [`WindowEvent::Magnify`], and turning about each other as [`WindowEvent::Rotate`], with
//! the totals in [`WindowEvent::Gesture`]. When one lifts, the other goes on scrolling.
//!
//! Only the mobile hosts feed touches; elsewhere the state machine is built but not reached.

#![cfg_attr(
    not(any(target_os = "ios", target_os = "android")),
    allow(dead_code, unused_imports)
)]

#[cfg(feature = "touch")]
mod mouse;
#[cfg(not(feature = "touch"))]
mod off;
#[cfg(feature = "touch")]
mod on;

#[cfg(not(feature = "touch"))]
pub(crate) use off::Touches;
#[cfg(feature = "touch")]
pub(crate) use on::Touches;

/// Whether touches are built in.
pub(crate) const fn built() -> bool {
    cfg!(feature = "touch")
}
