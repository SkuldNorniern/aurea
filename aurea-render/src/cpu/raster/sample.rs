//! Reading an image between its pixels: bilinear for enlarging, a box average for shrinking.

use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped};
use crate::types::Image;

/// The pixels a draw may read: its source rect, clamped to the image, as inclusive pixel indices.
#[derive(Clone, Copy)]
pub struct Bounds {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

fn premul(image: &Image, x: i32, y: i32) -> [f32; 4] {
    let (x, y) = (x.unsigned_abs() as usize, y.unsigned_abs() as usize);
    let i = (y * image.width as usize + x) * 4;
    let Some(px) = image.data.get(i..i + 4) else {
        return [0.0; 4];
    };
    let a = f32::from(px[3]) / 255.0;
    [
        f32::from(px[0]) * a,
        f32::from(px[1]) * a,
        f32::from(px[2]) * a,
        f32::from(px[3]),
    ]
}

fn pack(c: [f32; 4], alpha: f32) -> u32 {
    let a = c[3] * alpha;
    if a <= 0.0 {
        return 0;
    }
    let un = 255.0 / c[3];
    let byte = |v: f32| f32_to_u32_clamped((v + 0.5).clamp(0.0, 255.0));
    (byte(a) << 24) | (byte(c[0] * un) << 16) | (byte(c[1] * un) << 8) | byte(c[2] * un)
}

/// Nearest texel, clamped to the source region.
pub fn nearest(image: &Image, b: Bounds, u: f32, v: f32, alpha: f32) -> u32 {
    if b.x0 > b.x1 || b.y0 > b.y1 {
        return 0;
    }
    pack(
        premul(
            image,
            f32_to_i32_clamped(u.floor()).clamp(b.x0, b.x1),
            f32_to_i32_clamped(v.floor()).clamp(b.y0, b.y1),
        ),
        alpha,
    )
}

/// The two pixels a bilinear read blends along one axis, and how far toward the second it sits.
#[derive(Clone, Copy)]
pub struct Taps {
    lo: i32,
    hi: i32,
    t: f32,
    /// `t` in 256ths, for the integer read of opaque pixels.
    w: u32,
    /// `t` in 65536ths, for the integer read of translucent pixels: 256ths would move a
    /// low-alpha texel's colour by up to ~127 next to a high-alpha one.
    w16: u32,
}

impl Taps {
    /// The taps at pixel coordinate `u`, clamped to `lo..=hi`.
    pub fn at(u: f32, lo: i32, hi: i32) -> Self {
        let u = u - 0.5;
        let f = u.floor();
        let x = f32_to_i32_clamped(f);
        Self {
            lo: x.clamp(lo, hi),
            hi: (x + 1).clamp(lo, hi),
            t: u - f,
            w: f32_to_u32_clamped(((u - f) * 256.0 + 0.5).min(256.0)),
            w16: f32_to_u32_clamped(((u - f) * 65536.0 + 0.5).min(65536.0)),
        }
    }
}

/// Bilinear sample at image position `(u, v)`, pixel centres at `n + 0.5`.
pub fn bilinear(image: &Image, b: Bounds, u: f32, v: f32, alpha: f32) -> u32 {
    if b.x0 > b.x1 || b.y0 > b.y1 {
        return 0;
    }
    bilinear_taps(
        image,
        Taps::at(u, b.x0, b.x1),
        Taps::at(v, b.y0, b.y1),
        alpha,
    )
}

/// [`bilinear`] with its taps worked out, as a draw does once per column and row.
pub fn bilinear_taps(image: &Image, x: Taps, y: Taps, alpha: f32) -> u32 {
    let p00 = premul(image, x.lo, y.lo);
    let p10 = premul(image, x.hi, y.lo);
    let p01 = premul(image, x.lo, y.hi);
    let p11 = premul(image, x.hi, y.hi);
    let mut out = [0.0; 4];
    for (k, o) in out.iter_mut().enumerate() {
        let top = p00[k] + (p10[k] - p00[k]) * x.t;
        let bottom = p01[k] + (p11[k] - p01[k]) * x.t;
        *o = top + (bottom - top) * y.t;
    }
    pack(out, alpha)
}

/// [`bilinear_taps`] in integers, for texels of any alpha. Each colour is premultiplied exactly
/// (`c * a`) and weighted at 16 bits per tap; the sums are divided back out by the summed alpha,
/// so nothing is rounded before the last step. `af` is the draw's alpha in 256ths.
fn blend_translucent(texels: [u32; 4], x: Taps, y: Taps, af: u32) -> u32 {
    let (x1, y1) = (u64::from(x.w16), u64::from(y.w16));
    let (x0, y0) = (65536 - x1, 65536 - y1);
    let weights = [x0 * y0, x1 * y0, x0 * y1, x1 * y1];
    let (mut colour, mut alpha) = ([0u64; 3], 0u64);
    for (p, w) in texels.into_iter().zip(weights) {
        let a = u64::from(p >> 24);
        alpha += w * a;
        for (k, c) in colour.iter_mut().enumerate() {
            *c += w * a * u64::from((p >> (8 * k)) & 0xFF);
        }
    }
    if alpha == 0 {
        return 0;
    }
    // `alpha` is the blended alpha in 2^32 x (0..=255); scale by the draw's alpha in 2^8.
    let out_alpha = (alpha * u64::from(af) + (1 << 39)) >> 40;
    if out_alpha == 0 {
        return 0;
    }
    let half = alpha / 2;
    let channel = |k: usize| u8::try_from(((colour[k] + half) / alpha).min(255)).unwrap_or(u8::MAX);
    let a = u8::try_from(out_alpha).unwrap_or(u8::MAX);
    (u32::from(a) << 24)
        | (u32::from(channel(0)) << 16)
        | (u32::from(channel(1)) << 8)
        | u32::from(channel(2))
}

/// One row of [`bilinear_taps`] reads into `out`, one per column; returns whether all came out
/// opaque. Opaque texels at full alpha blend two channels per integer multiply; all others go
/// through [`blend_translucent`]. Both stay within 1 of the float read.
pub fn bilinear_row(image: &Image, columns: &[Taps], y: Taps, alpha: f32, out: &mut [u32]) -> bool {
    let stride = image.width as usize * 4;
    let row = |at: i32| {
        let start = at.unsigned_abs() as usize * stride;
        image.data.get(start..start + stride)
    };
    let (Some(top), Some(bottom)) = (row(y.lo), row(y.hi)) else {
        out.fill(0);
        return false;
    };
    let texel = |row: &[u8], at: i32| {
        let i = at.unsigned_abs() as usize * 4;
        row.get(i..i + 4)
            .map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
    };
    let af = f32_to_u32_clamped((alpha * 256.0 + 0.5).min(256.0));
    let mut all_opaque = true;
    for (o, x) in out.iter_mut().zip(columns) {
        let (Some(p00), Some(p10), Some(p01), Some(p11)) = (
            texel(top, x.lo),
            texel(top, x.hi),
            texel(bottom, x.lo),
            texel(bottom, x.hi),
        ) else {
            *o = 0;
            all_opaque = false;
            continue;
        };
        if alpha < 1.0 || p00 & p10 & p01 & p11 < 0xFF00_0000 {
            *o = blend_translucent([p00, p10, p01, p11], *x, y, af);
        } else {
            // Bytes are r, g, b, a: red and blue share one word, green the other.
            let lerp =
                |a: u32, b: u32, t: u32| ((a * (256 - t) + b * t + 0x0080_0080) >> 8) & 0x00FF_00FF;
            let pair = |shift: u32| {
                let m = |p: u32| (p >> shift) & 0x00FF_00FF;
                lerp(lerp(m(p00), m(p10), x.w), lerp(m(p01), m(p11), x.w), y.w)
            };
            let (rb, g) = (pair(0), pair(8) & 0xFF);
            *o = 0xFF00_0000 | ((rb & 0xFF) << 16) | (g << 8) | (rb >> 16);
        }
        all_opaque &= *o >> 24 == 255;
    }
    all_opaque
}

/// The pixels `[u0, u1)` covers along one axis, clamped to `lo..=hi`, each with the length of it
/// covered, as `area` weighs them.
pub fn area_weights(
    u0: f32,
    u1: f32,
    lo: i32,
    hi: i32,
) -> impl Iterator<Item = (i32, f32)> + Clone {
    let xs = f32_to_i32_clamped(u0.floor()).max(lo)..=(f32_to_i32_clamped(u1.ceil()) - 1).min(hi);
    xs.map(move |x| (x, ((x + 1) as f32).min(u1) - (x as f32).max(u0)))
}

/// The area-weighted average of the image over `[u0, u1) x [v0, v1)`.
pub fn area(
    image: &Image,
    b: Bounds,
    (u0, v0): (f32, f32),
    (u1, v1): (f32, f32),
    alpha: f32,
) -> u32 {
    area_from(
        image,
        area_weights(u0, u1, b.x0, b.x1),
        area_weights(v0, v1, b.y0, b.y1),
        alpha,
    )
}

/// [`area`] over weights from [`area_weights`], which a draw can work out once per column and row.
pub fn area_from(
    image: &Image,
    xs: impl Iterator<Item = (i32, f32)> + Clone,
    ys: impl Iterator<Item = (i32, f32)>,
    alpha: f32,
) -> u32 {
    let mut sum = [0.0f32; 4];
    let mut total = 0.0;
    for (y, wy) in ys {
        for (x, wx) in xs.clone() {
            let w = wy * wx;
            if w <= 0.0 {
                continue;
            }
            let p = premul(image, x, y);
            for k in 0..4 {
                sum[k] += p[k] * w;
            }
            total += w;
        }
    }
    if total <= 0.0 {
        return 0;
    }
    pack(sum.map(|s| s / total), alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker() -> Image {
        let w = [255, 255, 255, 255];
        let k = [0, 0, 0, 255];
        Image::new(2, 2, [w, k, k, w].concat())
    }

    const ALL: Bounds = Bounds {
        x0: 0,
        y0: 0,
        x1: 1,
        y1: 1,
    };

    #[test]
    fn bilinear_hits_pixel_centres_exactly() {
        let img = checker();
        assert_eq!(bilinear(&img, ALL, 0.5, 0.5, 1.0), 0xFFFF_FFFF);
        assert_eq!(bilinear(&img, ALL, 1.5, 0.5, 1.0), 0xFF00_0000);
        assert_eq!(bilinear(&img, ALL, 1.0, 1.0, 1.0), 0xFF80_8080);
    }

    #[test]
    fn area_averages_the_whole_footprint() {
        assert_eq!(
            area(&checker(), ALL, (0.0, 0.0), (2.0, 2.0), 1.0),
            0xFF80_8080
        );
    }

    #[test]
    fn transparent_pixels_do_not_tint() {
        let img = Image::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 0]);
        let b = Bounds {
            x0: 0,
            y0: 0,
            x1: 1,
            y1: 0,
        };
        let mid = bilinear(&img, b, 1.0, 0.5, 1.0);
        assert_eq!(mid & 0x00FF_FFFF, 0x00FF_0000, "no green: {mid:08x}");
        assert_eq!(mid >> 24, 0x80);
    }

    #[test]
    fn opaque_rows_stay_within_one_of_the_float_read() {
        let data: Vec<u8> = (0..=255u8)
            .flat_map(|i| {
                [
                    i.wrapping_mul(37),
                    i.wrapping_mul(91),
                    i.wrapping_mul(13),
                    255,
                ]
            })
            .collect();
        let img = Image::new(16, 16, data);
        let columns: Vec<_> = (0..40)
            .map(|x| Taps::at(x as f32 * 0.37 + 0.2, 0, 15))
            .collect();
        let mut row = vec![0; columns.len()];
        for y in 0..40 {
            let y = Taps::at(y as f32 * 0.41 + 0.1, 0, 15);
            assert!(bilinear_row(&img, &columns, y, 1.0, &mut row));
            for (&got, &x) in row.iter().zip(&columns) {
                let want = bilinear_taps(&img, x, y, 1.0);
                for shift in [0, 8, 16, 24] {
                    let (a, b) = ((got >> shift) & 255, (want >> shift) & 255);
                    assert!(a.abs_diff(b) <= 1, "{got:08x} vs {want:08x}");
                }
            }
        }
    }

    /// A `width` x `height` image whose alphas cycle through 0, 1, 7, 128, 254, 255.
    fn translucent(width: u32, height: u32) -> Image {
        const ALPHAS: [u8; 6] = [0, 1, 7, 128, 254, 255];
        let data: Vec<u8> = (0..width * height)
            .flat_map(|i| {
                let i = usize::try_from(i).unwrap_or(0);
                let a = ALPHAS[(i * 5) % 6];
                let b = u8::try_from(i % 256).unwrap_or(0);
                [
                    b.wrapping_mul(37),
                    b.wrapping_mul(91),
                    b.wrapping_mul(13),
                    a,
                ]
            })
            .collect();
        Image::new(width, height, data)
    }

    #[test]
    fn translucent_rows_stay_within_one_of_the_float_read() {
        let img = translucent(16, 16);
        let columns: Vec<_> = (0..40)
            .map(|x| Taps::at(x as f32 * 0.37 + 0.2, 0, 15))
            .collect();
        let mut row = vec![0; columns.len()];
        for alpha in [1.0, 0.5, 0.01] {
            for y in 0..40 {
                let y = Taps::at(y as f32 * 0.41 + 0.1, 0, 15);
                bilinear_row(&img, &columns, y, alpha, &mut row);
                for (&got, &x) in row.iter().zip(&columns) {
                    let want = bilinear_taps(&img, x, y, alpha);
                    let (ga, wa) = (got >> 24, want >> 24);
                    assert!(ga.abs_diff(wa) <= 1, "{alpha}: {got:08x} vs {want:08x}");
                    // Colour is meaningless at zero alpha.
                    if ga > 0 && wa > 0 {
                        for shift in [0, 8, 16] {
                            let (a, b) = ((got >> shift) & 255, (want >> shift) & 255);
                            assert!(a.abs_diff(b) <= 1, "{alpha}: {got:08x} vs {want:08x}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn translucent_row_read_timing() {
        use std::hint::black_box;
        use std::time::Instant;

        let img = translucent(256, 256);
        let columns: Vec<_> = (0..333)
            .map(|x| Taps::at(x as f32 * 0.77 + 0.3, 0, 255))
            .collect();
        let rows: Vec<_> = (0..256)
            .map(|y| Taps::at(y as f32 * 0.99 + 0.2, 0, 255))
            .collect();
        let mut row = vec![0; columns.len()];
        let float = || {
            let mut sum = 0u32;
            for &y in &rows {
                for &x in &columns {
                    sum = sum.wrapping_add(bilinear_taps(&img, x, y, 0.5));
                }
            }
            sum
        };
        let mut int = || {
            let mut sum = 0u32;
            for &y in &rows {
                bilinear_row(&img, &columns, y, 0.5, &mut row);
                sum = row.iter().fold(sum, |s, &p| s.wrapping_add(p));
            }
            sum
        };
        let best = |f: &mut dyn FnMut() -> u32| {
            (0..3)
                .map(|_| {
                    let start = Instant::now();
                    for _ in 0..20 {
                        black_box(f());
                    }
                    start.elapsed()
                })
                .min()
                .unwrap_or_default()
        };
        let (fl, it) = (best(&mut || float()), best(&mut int));
        let gain = 100.0 * (1.0 - it.as_secs_f64() / fl.as_secs_f64());
        println!("float {fl:?}, integer {it:?}, gain {gain:.1}%");
    }
}
