//! Narrowing helpers for the graph module.
//!
//! Sample data is `f64` and the drawing API is `f32`, so the conversion has to
//! happen somewhere. Doing it in one place keeps the `as` casts out of the
//! drawing and scaling code, where they are easy to get wrong.

/// `f64` to `f32`, saturating at the ends of the `f32` range.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn narrow(v: f64) -> f32 {
    v as f32
}

/// `usize` to `f64`. Exact for any count a plot can hold.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn count_to_f64(v: usize) -> f64 {
    v as f64
}

/// `f64` to `usize`, for values already clamped to a small non-negative range.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn f64_to_count(v: f64) -> usize {
    if v <= 0.0 { 0 } else { v as usize }
}

/// `f32` to `i32`, for a pixel column index.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn f32_to_i32(v: f32) -> i32 {
    v as i32
}
