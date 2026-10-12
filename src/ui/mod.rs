//! Widgets drawn on a canvas, styled by a [`Theme`] and the same on every
//! platform.
//!
//! They are immediate: each frame the draw callback says what is on screen
//! and where, and each widget answers what the input did to it. There is no
//! tree to keep in sync with the app's state, which stays the app's own.
//!

pub mod text_edit;
