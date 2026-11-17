//! Typed text, as `WindowEvent::TextInput`. Desktop windows deliver it with `deliver`; the mobile
//! hosts pass the text they read to `typed`.

#![cfg_attr(
    not(any(target_os = "ios", target_os = "android")),
    allow(dead_code, unused_imports)
)]

#[cfg(not(feature = "text-input"))]
mod off;
#[cfg(feature = "text-input")]
mod on;

#[cfg(not(feature = "text-input"))]
pub(crate) use off::{deliver, typed};
#[cfg(feature = "text-input")]
pub(crate) use on::{deliver, typed};

/// Whether typed text is built in.
pub(crate) const fn built() -> bool {
    cfg!(feature = "text-input")
}
