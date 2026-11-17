//! Input method composition, as `WindowEvent::ImePreedit`, and the candidate window's place.

#![cfg_attr(
    not(any(target_os = "ios", target_os = "android")),
    allow(dead_code, unused_imports)
)]

#[cfg(not(feature = "ime"))]
mod off;
#[cfg(feature = "ime")]
mod on;

#[cfg(not(feature = "ime"))]
pub(crate) use off::{composing, deliver, set_cursor_area};
#[cfg(feature = "ime")]
pub(crate) use on::{composing, deliver, set_cursor_area};

/// Whether input method composition is built in.
pub(crate) const fn built() -> bool {
    cfg!(feature = "ime")
}
