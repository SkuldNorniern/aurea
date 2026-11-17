//! Pinch, twist and two-finger scroll: `Magnify`, `Rotate`, `Gesture`. The math is shared with the
//! touch state machine on the mobile hosts.

#![cfg_attr(
    not(any(target_os = "ios", target_os = "android")),
    allow(dead_code, unused_imports)
)]

#[cfg(not(feature = "gestures"))]
mod off;
#[cfg(feature = "gestures")]
mod on;

#[cfg(not(feature = "gestures"))]
pub(crate) use off::deliver;
#[cfg(feature = "gestures")]
pub(crate) use on::{Pose, deliver, gesture, pose, scroll_edge, totals, wheel, wrap};

/// Whether gestures are built in.
pub(crate) const fn built() -> bool {
    cfg!(feature = "gestures")
}
