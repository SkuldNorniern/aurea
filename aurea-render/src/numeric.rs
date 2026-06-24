//! Float-to-integer conversion helpers.

#[inline]
pub(crate) fn f32_to_u8_clamped(v: f32) -> u8 {
    let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 255.0) };
    // SAFETY: v is clamped to u8::MIN..=u8::MAX (as f32) on the line above.
    unsafe { v.to_int_unchecked() }
}

#[inline]
pub(crate) fn f32_to_u32_clamped(v: f32) -> u32 {
    let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 16_777_216.0) };
    // SAFETY: v is clamped to within u32's range (and exactly representable) above.
    unsafe { v.to_int_unchecked() }
}

#[inline]
pub(crate) fn f32_to_usize_clamped(v: f32) -> usize {
    let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 16_777_216.0) };
    // SAFETY: v is clamped to within usize's range (and exactly representable) above.
    unsafe { v.to_int_unchecked() }
}

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
