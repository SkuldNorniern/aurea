//! One pixel at a time: colours as words, and writes that respect the clip.

use crate::cpu::blend::blend_pixel;
use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::numeric::{f32_to_u8_clamped, f32_to_u32_clamped};
use crate::types::{BlendMode, Color, Paint};

#[inline]
pub(crate) fn put(
    buf: &mut Band<'_>,
    w: u32,
    clip: ClipBox,
    x: i32,
    y: i32,
    color: u32,
    mode: BlendMode,
) {
    if !clip.contains(x, y) {
        return;
    }
    let idx = y.unsigned_abs() * w + x.unsigned_abs();
    if idx as usize >= buf.len() {
        return;
    }
    if mode == BlendMode::Normal && (color >> 24) == 255 {
        buf[idx as usize] = color;
    } else {
        let dst = buf[idx as usize];
        buf[idx as usize] = blend_pixel(color, dst, mode);
    }
}

pub(crate) fn color_to_u32(c: Color) -> u32 {
    (u32::from(c.a) << 24) | (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
}

pub(crate) fn color_to_u32_with_coverage(c: Color, cov: f32) -> u32 {
    let a = f32_to_u32_clamped((f32::from(c.a) * cov).round());
    (a << 24) | (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
}

pub(crate) fn bytes_fit(width: u32, height: u32, per_pixel: usize, len: usize) -> bool {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(per_pixel))
        .is_some_and(|need| len >= need)
}

pub(crate) fn scale_u8(v: u8, factor: f32) -> u8 {
    if factor >= 1.0 {
        return v;
    }
    f32_to_u8_clamped((f32::from(v) * factor).round())
}

pub(crate) fn fade_color(c: Color, alpha: f32) -> Color {
    if alpha >= 1.0 {
        return c;
    }
    Color {
        a: f32_to_u8_clamped((f32::from(c.a) * alpha).round()),
        ..c
    }
}

pub(crate) fn fade_paint(paint: &Paint, alpha: f32) -> Paint {
    if alpha >= 1.0 {
        return paint.clone();
    }
    let mut faded = paint.clone();
    faded.color = fade_color(paint.color, alpha);
    faded
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn pixel_at(buf: &[u32], w: u32, x: u32, y: u32) -> u32 {
    let idx = (y * w + x) as usize;
    if idx < buf.len() { buf[idx] } else { 0 }
}
