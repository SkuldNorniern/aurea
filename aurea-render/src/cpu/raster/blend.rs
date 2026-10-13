//! Pixel-level blend modes for compositing.
//!
//! Colors are packed as RGBA u32: alpha in the high byte, then red, green, blue.
//! Each mode combines a source pixel with the existing destination pixel to produce
//! the final color (e.g. Normal is "over", Multiply darkens, Screen lightens).
//! Alpha uses source-over for every blend mode, with straight RGB normalized
//! by the resulting alpha. Blend functions apply only where both layers overlap.
//! See <https://www.w3.org/TR/compositing-1/#generalformula>.

use crate::numeric::{f32_to_u8_clamped, f32_to_u32_clamped, f32_to_usize_clamped};
use crate::types::BlendMode;
use std::sync::LazyLock;

/// sRGB (0..=255) -> linear-light (0.0..=1.0) lookup table.
///
/// Antialiased coverage — text edges in particular — must be composited in
/// linear light, not in the gamma-encoded sRGB values the framebuffer stores.
/// Lerping partial-coverage edge pixels directly in sRGB makes light-on-dark
/// text look thin and the curves/stems look jagged; doing the lerp in linear
/// light and re-encoding yields smooth, full-weight edges.
static SRGB_TO_LINEAR: LazyLock<[f32; 256]> = LazyLock::new(|| {
    let mut lut = [0.0f32; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let c = i as f32 / 255.0;
        *slot = if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        };
    }
    lut
});

/// sRGB-encoded 8-bit channel value -> linear light (0.0..=1.0).
pub fn srgb_to_linear(c: u8) -> f32 {
    SRGB_TO_LINEAR[c as usize]
}

/// Linear-light [0,1] → sRGB 8-bit, using a 4096-entry lookup table.
///
/// Avoids a `powf` call per pixel; the quantisation error is < 0.5 ULP at u8
/// precision (the final value is rounded to u8 anyway).
static LINEAR_TO_SRGB: LazyLock<[u8; 4097]> = LazyLock::new(|| {
    let mut lut = [0u8; 4097];
    for (i, slot) in lut.iter_mut().enumerate() {
        let c = i as f32 / 4096.0;
        let s = if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        *slot = f32_to_u8_clamped((s * 255.0).round());
    }
    lut
});

/// Linear-light (0.0..=1.0) -> sRGB-encoded 8-bit channel value.
#[inline]
pub fn linear_to_srgb_u8(c: f32) -> u32 {
    let idx = f32_to_usize_clamped(c.clamp(0.0, 1.0) * 4096.0);
    u32::from(LINEAR_TO_SRGB[idx])
}

/// sRGB 8-bit -> linear light scaled to `0..=4096`, for compositing with
/// integers where a span of pixels each mix by their own coverage.
static SRGB_TO_LINEAR_12: LazyLock<[u16; 256]> = LazyLock::new(|| {
    let mut lut = [0u16; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let linear = SRGB_TO_LINEAR[i];
        *slot = u16::try_from(f32_to_u32_clamped((linear * 4096.0).round())).unwrap_or(4096);
    }
    lut
});

/// Both linear-light tables, looked up once for a run of pixels rather than
/// through their lazy statics per channel.
#[derive(Clone, Copy)]
pub struct LinearLuts {
    to_linear: &'static [u16; 256],
    to_srgb: &'static [u8; 4097],
}

impl LinearLuts {
    pub fn get() -> Self {
        Self {
            to_linear: &SRGB_TO_LINEAR_12,
            to_srgb: &LINEAR_TO_SRGB,
        }
    }

    /// `c` in linear light, `0..=4096`.
    #[inline]
    pub fn linear(self, c: u8) -> u32 {
        u32::from(self.to_linear[usize::from(c)])
    }

    /// `src` (linear, `0..=4096`) over `dst` (sRGB) by `coverage` out of
    /// 255, mixed in linear light and encoded back to sRGB.
    #[inline]
    pub fn mix(self, src: u32, dst: u8, coverage: u8) -> u32 {
        let c = u32::from(coverage);
        let d = self.linear(dst);
        let v = (src * c + d * (255 - c) + 127) / 255;
        u32::from(self.to_srgb[(v as usize).min(4096)])
    }
}

/// Precomputed linear-light source channels for a constant-color span.
///
/// When a draw call fills a span with the same source color for many pixels,
/// computing the linear source channels once per span (instead of once per
/// pixel inside `blend_over`) saves 3 LUT lookups per pixel.
///
/// Only worth using for `BlendMode::Normal` with `sa < 255`; for the opaque
/// fast path (`sa == 255`) callers already skip blending entirely.
#[derive(Clone, Copy)]
pub struct ConstSrc {
    rgb: u32,
    pub sa: u32,
    pub da_scale: f32, // (255 - sa) / 255 — constant for alpha composition
    pub lr: f32,       // srgb_to_linear(src.r)
    pub lg: f32,       // srgb_to_linear(src.g)
    pub lb: f32,       // srgb_to_linear(src.b)
    pub cov: f32,      // sa as f32 / 255.0
}

impl ConstSrc {
    #[inline]
    pub fn new(src: u32) -> Self {
        let sa = sa(src);
        let cov = sa as f32 / 255.0;
        Self {
            rgb: src & 0x00ff_ffff,
            sa,
            da_scale: (255 - sa) as f32 / 255.0,
            lr: srgb_to_linear(sr8(src)),
            lg: srgb_to_linear(sg8(src)),
            lb: srgb_to_linear(sb8(src)),
            cov,
        }
    }

    /// Composite this source at a per-pixel alpha, in `0..=255`.
    ///
    /// The linear source channels do not depend on alpha, so a shape whose
    /// colour is constant but whose coverage varies — the antialiased edge of
    /// any fill, and every pixel of a hairline that never fully covers one —
    /// still avoids three `srgb_to_linear` lookups per pixel.
    #[inline]
    pub fn over_at_alpha(self, dst: u32, alpha: u32) -> u32 {
        if alpha == 0 {
            return dst;
        }
        let da = da(dst);
        if alpha == 255 || da == 0 {
            return (alpha << 24) | self.rgb;
        }
        let out_a = alpha + ((255 - alpha) * da) / 255;
        if out_a == 0 {
            return 0;
        }
        let cov = if da == 255 {
            alpha as f32 / 255.0
        } else {
            // Store straight RGB: normalize the premultiplied source-over
            // result by its alpha instead of mixing with invisible RGB.
            (alpha * 255) as f32 / (alpha * 255 + (255 - alpha) * da) as f32
        };
        let inv = 1.0 - cov;
        let out_r = linear_to_srgb_u8(self.lr * cov + srgb_to_linear(dr8(dst)) * inv);
        let out_g = linear_to_srgb_u8(self.lg * cov + srgb_to_linear(dg8(dst)) * inv);
        let out_b = linear_to_srgb_u8(self.lb * cov + srgb_to_linear(db8(dst)) * inv);
        (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
    }

    /// Composite this constant source onto a single destination pixel.
    /// Equivalent to `blend_over(src, dst)` but skips the per-pixel source
    /// `srgb_to_linear` calls.
    #[inline]
    pub fn over(self, dst: u32) -> u32 {
        self.over_at_alpha(dst, self.sa)
    }
}

/// Composites a source pixel onto a destination pixel using the given blend mode.
/// Returns the resulting color as RGBA u32.
pub fn blend_pixel(src: u32, dst: u32, mode: BlendMode) -> u32 {
    if mode == BlendMode::Normal {
        return blend_over(src, dst);
    }
    let source_alpha = sa(src);
    let dest_alpha = da(dst);
    if source_alpha == 0 {
        return dst;
    }
    if dest_alpha == 0 {
        return src;
    }
    let blended = blend_opaque(src | 0xff00_0000, dst | 0xff00_0000, mode);
    if source_alpha == 255 && dest_alpha == 255 {
        return blended;
    }
    // Blend only the overlap; composite source-only and backdrop-only regions
    // with source-over, then normalize back to straight RGB (W3C compositing).
    let overlap = source_alpha * dest_alpha;
    let source_only = source_alpha * (255 - dest_alpha);
    let dest_only = dest_alpha * (255 - source_alpha);
    let total = overlap + source_only + dest_only;
    let channel = |s: u32, d: u32, b: u32| {
        (s * source_only + d * dest_only + b * overlap + total / 2) / total
    };
    ((total / 255) << 24)
        | (channel(sr(src), dr(dst), sr(blended)) << 16)
        | (channel(sg(src), dg(dst), sg(blended)) << 8)
        | channel(sb(src), db(dst), sb(blended))
}

fn blend_opaque(src: u32, dst: u32, mode: BlendMode) -> u32 {
    match mode {
        BlendMode::Normal => blend_over(src, dst),
        BlendMode::Multiply => blend_multiply(src, dst),
        BlendMode::Screen => blend_screen(src, dst),
        BlendMode::Overlay => blend_overlay(src, dst),
        BlendMode::Darken => blend_darken(src, dst),
        BlendMode::Lighten => blend_lighten(src, dst),
        BlendMode::ColorDodge => blend_color_dodge(src, dst),
        BlendMode::ColorBurn => blend_color_burn(src, dst),
        BlendMode::HardLight => blend_hard_light(src, dst),
        BlendMode::SoftLight => blend_soft_light(src, dst),
        BlendMode::Difference => blend_difference(src, dst),
        BlendMode::Exclusion => blend_exclusion(src, dst),
    }
}

fn sr(src: u32) -> u32 {
    (src >> 16) & 0xff
}
fn sg(src: u32) -> u32 {
    (src >> 8) & 0xff
}
fn sb(src: u32) -> u32 {
    src & 0xff
}
fn sa(src: u32) -> u32 {
    (src >> 24) & 0xff
}
fn sr8(src: u32) -> u8 {
    ((src >> 16) & 0xff) as u8
}
fn sg8(src: u32) -> u8 {
    ((src >> 8) & 0xff) as u8
}
fn sb8(src: u32) -> u8 {
    (src & 0xff) as u8
}
fn dr8(dst: u32) -> u8 {
    ((dst >> 16) & 0xff) as u8
}
fn dg8(dst: u32) -> u8 {
    ((dst >> 8) & 0xff) as u8
}
fn db8(dst: u32) -> u8 {
    (dst & 0xff) as u8
}
fn dr(dst: u32) -> u32 {
    (dst >> 16) & 0xff
}
fn dg(dst: u32) -> u32 {
    (dst >> 8) & 0xff
}
fn db(dst: u32) -> u32 {
    dst & 0xff
}
fn da(dst: u32) -> u32 {
    (dst >> 24) & 0xff
}

fn blend_over(src: u32, dst: u32) -> u32 {
    let sa = sa(src);
    if sa >= 255 {
        return src;
    }
    if sa == 0 {
        return dst;
    }
    ConstSrc::new(src).over(dst)
}

fn blend_multiply(src: u32, dst: u32) -> u32 {
    let sa = sa(src);
    let da = da(dst);
    let out_a = (sa * da) / 255;
    if out_a == 0 {
        return 0;
    }
    let out_r = (sr(src) * dr(dst)) / 255;
    let out_g = (sg(src) * dg(dst)) / 255;
    let out_b = (sb(src) * db(dst)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_screen(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = 255 - ((255 - sr) * (255 - dr)) / 255;
    let out_g = 255 - ((255 - sg) * (255 - dg)) / 255;
    let out_b = 255 - ((255 - sb) * (255 - db)) / 255;
    let out_a = 255 - ((255 - sa) * (255 - da)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_overlay(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = overlay_channel(sr, dr);
    let out_g = overlay_channel(sg, dg);
    let out_b = overlay_channel(sb, db);
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn overlay_channel(s: u32, d: u32) -> u32 {
    if d < 128 {
        (2 * s * d) / 255
    } else {
        255 - (2 * (255 - s) * (255 - d)) / 255
    }
}

fn blend_darken(src: u32, dst: u32) -> u32 {
    let sa = sa(src);
    let da = da(dst);
    let out_a = sa + (da * (255 - sa)) / 255;
    if out_a == 0 {
        return 0;
    }
    let out_r = sr(src).min(dr(dst));
    let out_g = sg(src).min(dg(dst));
    let out_b = sb(src).min(db(dst));
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_lighten(src: u32, dst: u32) -> u32 {
    let sa = sa(src);
    let da = da(dst);
    let out_a = sa + (da * (255 - sa)) / 255;
    if out_a == 0 {
        return 0;
    }
    let out_r = sr(src).max(dr(dst));
    let out_g = sg(src).max(dg(dst));
    let out_b = sb(src).max(db(dst));
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_color_dodge(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = if sr >= 255 {
        255
    } else {
        (255 * dr).min(255) / (255 - sr)
    };
    let out_g = if sg >= 255 {
        255
    } else {
        (255 * dg).min(255) / (255 - sg)
    };
    let out_b = if sb >= 255 {
        255
    } else {
        (255 * db).min(255) / (255 - sb)
    };
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_color_burn(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = ((255 - dr) * 255)
        .min(255)
        .checked_div(sr)
        .map_or(0, |v| 255 - v);
    let out_g = ((255 - dg) * 255)
        .min(255)
        .checked_div(sg)
        .map_or(0, |v| 255 - v);
    let out_b = ((255 - db) * 255)
        .min(255)
        .checked_div(sb)
        .map_or(0, |v| 255 - v);
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_hard_light(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = overlay_channel(dr, sr);
    let out_g = overlay_channel(dg, sg);
    let out_b = overlay_channel(db, sb);
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_soft_light(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = soft_light_channel(sr, dr);
    let out_g = soft_light_channel(sg, dg);
    let out_b = soft_light_channel(sb, db);
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn soft_light_channel(s: u32, d: u32) -> u32 {
    let s = s as f32 / 255.0;
    let d = d as f32 / 255.0;
    let r = if s <= 0.5 {
        d * (1.0 - (1.0 - 2.0 * s) * (1.0 - d))
    } else {
        d * (1.0
            + (2.0 * s - 1.0)
                * (if d <= 0.25 {
                    ((16.0 * d - 12.0) * d + 4.0) * d
                } else {
                    d.sqrt() - d
                }))
    };
    f32_to_u32_clamped(r * 255.0)
}

fn blend_difference(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = sr.abs_diff(dr);
    let out_g = sg.abs_diff(dg);
    let out_b = sb.abs_diff(db);
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_exclusion(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = sr + dr - (2 * sr * dr) / 255;
    let out_g = sg + dg - (2 * sg * dg) / 255;
    let out_b = sb + db - (2 * sb * db) / 255;
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BlendMode;

    fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
        (u32::from(a) << 24) | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
    }

    const MODES: [BlendMode; 12] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
    ];

    #[test]
    fn transparent_colors_do_not_affect_compositing() {
        let src = rgba(210, 50, 130, 128);
        let dst = rgba(30, 160, 90, 96);
        for mode in MODES {
            assert_eq!(
                blend_pixel(rgba(250, 10, 200, 0), dst, mode),
                dst,
                "{mode:?}"
            );
            assert_eq!(
                blend_pixel(src, rgba(10, 250, 20, 0), mode),
                src,
                "{mode:?}"
            );
        }
    }

    #[test]
    fn blend_modes_use_source_over_alpha() {
        for mode in MODES {
            for source_alpha in [1u8, 64, 128, 254, 255] {
                for dest_alpha in [1u8, 96, 128, 254, 255] {
                    let actual = blend_pixel(
                        rgba(200, 80, 20, source_alpha),
                        rgba(20, 100, 220, dest_alpha),
                        mode,
                    );
                    let expected = u32::from(source_alpha)
                        + (255 - u32::from(source_alpha)) * u32::from(dest_alpha) / 255;
                    assert_eq!(actual >> 24, expected, "{mode:?}");
                }
            }
        }
    }

    #[test]
    fn translucent_multiply_blends_only_the_overlap() {
        assert_eq!(
            blend_pixel(
                rgba(255, 0, 0, 128),
                rgba(0, 0, 255, 128),
                BlendMode::Multiply
            ),
            rgba(85, 0, 85, 191)
        );
    }

    #[test]
    fn normal_translucent_layers_preserve_straight_rgb() {
        let src = rgba(255, 0, 0, 128);
        assert_eq!(blend_pixel(src, 0, BlendMode::Normal), src);
        let result = blend_pixel(src, rgba(0, 0, 255, 128), BlendMode::Normal);
        assert_eq!(result >> 24, 191);
        assert!(sr(result).abs_diff(213) <= 1, "{result:#010x}");
        assert!(sb(result).abs_diff(156) <= 1, "{result:#010x}");
        assert_eq!(ConstSrc::new(src).over(0), src);
    }

    /// Hoisting the source colour out of the per-pixel loop has to produce the
    /// same pixels, or it is not an optimisation but a change of appearance.
    #[test]
    fn hoisting_the_source_colour_changes_nothing() {
        let destinations = [
            rgba(0, 0, 0, 255),
            rgba(255, 255, 255, 255),
            rgba(30, 33, 40, 255),
            rgba(120, 200, 255, 128),
            rgba(0, 0, 0, 0),
        ];

        for (r, g, b) in [(120u8, 200u8, 255u8), (255, 0, 0), (10, 200, 40)] {
            for alpha in [1u8, 26, 64, 128, 200, 254] {
                let src = rgba(r, g, b, alpha);
                let hoisted = ConstSrc::new(rgba(r, g, b, 255));

                for dst in destinations {
                    let direct = blend_pixel(src, dst, BlendMode::Normal);
                    let via_const = hoisted.over_at_alpha(dst, u32::from(alpha));
                    assert_eq!(
                        direct, via_const,
                        "src {r},{g},{b} at alpha {alpha} over {dst:#010x}"
                    );
                }
            }
        }
    }

    #[test]
    fn zero_alpha_leaves_the_destination_alone() {
        let dst = rgba(30, 33, 40, 255);
        let src = ConstSrc::new(rgba(255, 0, 0, 255));
        assert_eq!(src.over_at_alpha(dst, 0), dst);
    }

    #[test]
    fn full_alpha_replaces_the_destination() {
        let dst = rgba(30, 33, 40, 255);
        let src = ConstSrc::new(rgba(255, 0, 0, 255));
        let out = src.over_at_alpha(dst, 255);
        assert_eq!(out >> 24, 255, "opaque result");
        assert_eq!((out >> 16) & 0xFF, 255, "the source colour");
    }
}
