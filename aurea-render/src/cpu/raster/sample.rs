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

/// Bilinear sample at image position `(u, v)`, pixel centres at `n + 0.5`.
pub fn bilinear(image: &Image, b: Bounds, u: f32, v: f32, alpha: f32) -> u32 {
    if b.x0 > b.x1 || b.y0 > b.y1 {
        return 0;
    }
    let (u, v) = (u - 0.5, v - 0.5);
    let (fx, fy) = (u.floor(), v.floor());
    let (tx, ty) = (u - fx, v - fy);
    let (x, y) = (f32_to_i32_clamped(fx), f32_to_i32_clamped(fy));
    let cx = |x: i32| x.clamp(b.x0, b.x1);
    let cy = |y: i32| y.clamp(b.y0, b.y1);
    let p00 = premul(image, cx(x), cy(y));
    let p10 = premul(image, cx(x + 1), cy(y));
    let p01 = premul(image, cx(x), cy(y + 1));
    let p11 = premul(image, cx(x + 1), cy(y + 1));
    let mut out = [0.0; 4];
    for (k, o) in out.iter_mut().enumerate() {
        let top = p00[k] + (p10[k] - p00[k]) * tx;
        let bottom = p01[k] + (p11[k] - p01[k]) * tx;
        *o = top + (bottom - top) * ty;
    }
    pack(out, alpha)
}

/// The area-weighted average of the image over `[u0, u1) x [v0, v1)`.
pub fn area(
    image: &Image,
    b: Bounds,
    (u0, v0): (f32, f32),
    (u1, v1): (f32, f32),
    alpha: f32,
) -> u32 {
    let xs =
        f32_to_i32_clamped(u0.floor()).max(b.x0)..=(f32_to_i32_clamped(u1.ceil()) - 1).min(b.x1);
    let ys =
        f32_to_i32_clamped(v0.floor()).max(b.y0)..=(f32_to_i32_clamped(v1.ceil()) - 1).min(b.y1);
    let mut sum = [0.0f32; 4];
    let mut total = 0.0;
    for y in ys {
        let wy = ((y + 1) as f32).min(v1) - (y as f32).max(v0);
        for x in xs.clone() {
            let w = wy * (((x + 1) as f32).min(u1) - (x as f32).max(u0));
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
}
