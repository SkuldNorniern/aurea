//! Keyboard, pointer, pen and touch input, and the shortcuts they drive.

pub(crate) mod keyboard;
pub mod keymap;
pub(crate) mod pen;
#[cfg(any(target_os = "ios", target_os = "android", test))]
pub(crate) mod touch;
