//! Float-to-integer conversion helpers.
//!
//! Rust's standard library has no safe, non-`as` way to convert a float to an
//! integer type, even when the caller has already clamped the value to fit.
//! Clippy's `cast_possible_truncation`/`cast_sign_loss` lints fire on every
//! `as` float-to-int cast regardless of a preceding `.clamp()` — there is no
//! literal-bound exception for float sources (only for int-to-int casts).
//!
//! These helpers centralize the unsafe conversion in one audited place per
//! destination type: each clamps to the destination's exact range first,
//! then uses `to_int_unchecked`, which is sound precisely because of that
//! clamp and is also faster than `as` (skips the redundant range check `as`
//! performs for NaN/out-of-range floats, which can't occur here).

/// Convert `v` to `u8`, clamping to `[0, 255]` first (NaN clamps to 0).
#[inline]
pub(crate) fn f32_to_u8_clamped(v: f32) -> u8 {
    let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 255.0) };
    // SAFETY: v is clamped to u8::MIN..=u8::MAX (as f32) on the line above.
    unsafe { v.to_int_unchecked() }
}

/// Convert `v` to `u32`, clamping to `[0, 2^24]` first (NaN clamps to 0).
///
/// The bound is `2^24` (16,777,216) rather than `u32::MAX` because that is
/// the largest integer exactly representable in an `f32`'s 24-bit mantissa —
/// well beyond any real pixel coordinate or buffer index, and exact so the
/// clamp bound itself can't round up past the destination type's range.
#[inline]
pub(crate) fn f32_to_u32_clamped(v: f32) -> u32 {
    let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 16_777_216.0) };
    // SAFETY: v is clamped to within u32's range (and exactly representable) above.
    unsafe { v.to_int_unchecked() }
}

/// Convert `v` to `usize`, clamping to `[0, 2^24]` first (NaN clamps to 0).
#[inline]
pub(crate) fn f32_to_usize_clamped(v: f32) -> usize {
    let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 16_777_216.0) };
    // SAFETY: v is clamped to within usize's range (and exactly representable) above.
    unsafe { v.to_int_unchecked() }
}

/// Convert `v` to `i32`, clamping to `[-2^24, 2^24]` first (NaN clamps to 0).
#[inline]
pub(crate) fn f32_to_i32_clamped(v: f32) -> i32 {
    let v = if v.is_nan() {
        0.0
    } else {
        v.clamp(-16_777_216.0, 16_777_216.0)
    };
    // SAFETY: v is clamped to within i32's range (and exactly representable) above.
    unsafe { v.to_int_unchecked() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(f32_to_u8_clamped(-5.0), 0);
        assert_eq!(f32_to_u8_clamped(300.0), 255);
        assert_eq!(f32_to_u8_clamped(f32::NAN), 0);
        assert_eq!(f32_to_u32_clamped(-1.0), 0);
        assert_eq!(f32_to_i32_clamped(-1.0), -1);
    }

    #[test]
    fn preserves_in_range_values() {
        assert_eq!(f32_to_u8_clamped(128.4), 128);
        assert_eq!(f32_to_u32_clamped(42.9), 42);
        assert_eq!(f32_to_usize_clamped(7.0), 7);
        assert_eq!(f32_to_i32_clamped(-7.9), -7);
    }
}
