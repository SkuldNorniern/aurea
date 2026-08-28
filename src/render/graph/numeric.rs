//! Narrowing helpers for the graph module.

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn narrow(v: f64) -> f32 {
    v as f32
}

#[allow(clippy::cast_precision_loss)]
pub(crate) fn count_to_f64(v: usize) -> f64 {
    v as f64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn f64_to_count(v: f64) -> usize {
    if v <= 0.0 { 0 } else { v as usize }
}
