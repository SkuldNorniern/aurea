//! Images: copied, scaled, filtered and drawn through a quad.

use crate::cpu::blend::blend_pixel;
use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::cpu::raster::mask::ClipScratch;
use crate::cpu::raster::{pixel, sample};
use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped, f32_to_usize_clamped};
use crate::scene::command::quad_bounds;
use crate::types::{BlendMode, Image, Point, Rect, Sampling};

#[derive(Default)]
pub(crate) struct ImageScratch {
    pub(crate) premul: sample::PremulCache,
    pub(crate) row: Vec<u32>,
    pub(crate) shrink: sample::Shrink,
    pub(crate) taps: Vec<sample::Taps>,
    /// Byte offset of each output column's source pixel, for nearest sampling.
    pub(crate) source_cols: Vec<usize>,
    pub(crate) clip: ClipScratch,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_quad(
    image: &Image,
    src: Rect,
    q: [Point; 3],
    sampling: Sampling,
    mode: BlendMode,
    alpha: f32,
    buf: &mut Band<'_>,
    scratch: &mut ImageScratch,
    prepare: bool,
    bw: u32,
    clip: ClipBox,
) {
    if image.width == 0
        || image.height == 0
        || !pixel::bytes_fit(image.width, image.height, 4, image.data.len())
        || ![src.x, src.y, src.width, src.height]
            .iter()
            .all(|v| v.is_finite())
        || src.width <= 0.0
        || src.height <= 0.0
        || !q.iter().all(|p| p.x.is_finite() && p.y.is_finite())
    {
        return;
    }
    let ax = q[1].x - q[0].x;
    let ay = q[1].y - q[0].y;
    let bx = q[2].x - q[0].x;
    let by = q[2].y - q[0].y;
    let det = ax * by - ay * bx;
    if !det.is_finite() || det.abs() < 1e-12 {
        return;
    }
    let inverse = det.recip();
    let b = quad_bounds(&q);
    let x0 = f32_to_i32_clamped(b.x.floor().max(clip.left()));
    let y0 = f32_to_i32_clamped(b.y.floor().max(clip.top()));
    let x1 = f32_to_i32_clamped(b.right().ceil().min(clip.right()));
    let y1 = f32_to_i32_clamped(b.bottom().ceil().min(clip.bottom()));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let iw = i32::try_from(image.width).unwrap_or(i32::MAX);
    let ih = i32::try_from(image.height).unwrap_or(i32::MAX);
    let bounds = sample::Bounds {
        x0: f32_to_i32_clamped(src.x.floor()).clamp(0, iw - 1),
        y0: f32_to_i32_clamped(src.y.floor()).clamp(0, ih - 1),
        x1: (f32_to_i32_clamped(src.right().ceil()).saturating_sub(1)).clamp(0, iw - 1),
        y1: (f32_to_i32_clamped(src.bottom().ceil()).saturating_sub(1)).clamp(0, ih - 1),
    };
    let footprint_x = ((by.abs() + bx.abs()) / det.abs() * src.width).max(1.0);
    let footprint_y = ((ay.abs() + ax.abs()) / det.abs() * src.height).max(1.0);
    let reads_per_pixel = if footprint_x > 1.0 || footprint_y > 1.0 {
        (footprint_x.ceil() + 1.0) * (footprint_y.ceil() + 1.0)
    } else {
        4.0
    };
    let reads = f32_to_usize_clamped(
        ((x1 - x0).max(0) as f32) * ((y1 - y0).max(0) as f32) * reads_per_pixel,
    );
    // Only area reads gain from floats kept per texel: enlarging reads bytes in integers.
    let shrinking = footprint_x > 1.0 || footprint_y > 1.0;
    let cached = if prepare && shrinking && sampling == Sampling::Smooth {
        scratch.premul.prepare(image, reads)
    } else {
        None
    };
    let span = (x0, y0, x1, y1);
    if let Some(pixels) = cached {
        sample_quad_pixels(q, src, inverse, span, buf, bw, clip, mode, |u, v| {
            sample::area_prepared(
                pixels,
                image.width,
                bounds,
                (u - footprint_x * 0.5, v - footprint_y * 0.5),
                (u + footprint_x * 0.5, v + footprint_y * 0.5),
                alpha,
            )
        });
    } else {
        sample_quad_pixels(
            q,
            src,
            inverse,
            span,
            buf,
            bw,
            clip,
            mode,
            |u, v| match sampling {
                Sampling::Nearest => sample::nearest(image, bounds, u, v, alpha),
                Sampling::Smooth if shrinking => sample::area(
                    image,
                    bounds,
                    (u - footprint_x * 0.5, v - footprint_y * 0.5),
                    (u + footprint_x * 0.5, v + footprint_y * 0.5),
                    alpha,
                ),
                _ => sample::bilinear_fast(image, bounds, u, v, alpha),
            },
        );
    }
}

/// The columns of row `y` that can fall inside the quad, a pixel wider than exact on each side: the
/// per-pixel test still decides, this only saves the misses.
fn quad_row_span(q: [Point; 3], inverse: f32, y: i32, (x0, x1): (i32, i32)) -> (i32, i32) {
    let dy = y as f32 + 0.5 - q[0].y;
    let (ax, ay, bx, by) = (
        q[1].x - q[0].x,
        q[1].y - q[0].y,
        q[2].x - q[0].x,
        q[2].y - q[0].y,
    );
    // u = (dx * by - dy * bx) * inverse and v = (dy * ax - dx * ay) * inverse, both in 0..1.
    let mut span = (f32::MIN, f32::MAX);
    for (slope, base) in [
        (by * inverse, -dy * bx * inverse),
        (-ay * inverse, dy * ax * inverse),
    ] {
        if slope.abs() < 1e-9 {
            if !(-1e-3..=1.001).contains(&base) {
                return (x0, x0);
            }
            continue;
        }
        let (a, b) = (-base / slope, (1.0 - base) / slope);
        span = (span.0.max(a.min(b)), span.1.min(a.max(b)));
    }
    let origin = q[0].x - 0.5;
    (
        f32_to_i32_clamped((span.0 + origin).floor() - 1.0).clamp(x0, x1),
        f32_to_i32_clamped((span.1 + origin).ceil() + 1.0).clamp(x0, x1),
    )
}

#[allow(clippy::too_many_arguments)]
fn sample_quad_pixels(
    q: [Point; 3],
    src: Rect,
    inverse: f32,
    (x0, y0, x1, y1): (i32, i32, i32, i32),
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
    mode: BlendMode,
    sample: impl Fn(f32, f32) -> u32,
) {
    let (ax, ay, bx, by) = (
        q[1].x - q[0].x,
        q[1].y - q[0].y,
        q[2].x - q[0].x,
        q[2].y - q[0].y,
    );
    for y in y0..y1 {
        let (from, to) = quad_row_span(q, inverse, y, (x0, x1));
        for x in from..to {
            let dx = x as f32 + 0.5 - q[0].x;
            let dy = y as f32 + 0.5 - q[0].y;
            let u = (dx * by - dy * bx) * inverse;
            let v = (dy * ax - dx * ay) * inverse;
            if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                continue;
            }
            let u = src.x + u * src.width;
            let v = src.y + v * src.height;
            let c = sample(u, v);
            if c >> 24 != 0 {
                pixel::put(buf, bw, clip, x, y, c, mode);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    image: &Image,
    src: Rect,
    dest: Rect,
    sampling: Sampling,
    mode: BlendMode,
    alpha: f32,
    buf: &mut Band<'_>,
    scratch: &mut ImageScratch,
    bw: u32,
    clip: ClipBox,
) {
    if image.width == 0
        || image.height == 0
        || !pixel::bytes_fit(image.width, image.height, 4, image.data.len())
        || !(src.width > 0.0 && src.height > 0.0)
        || dest.width <= 0.0
        || dest.height <= 0.0
    {
        return;
    }
    let centre_span = |from: f32, to: f32, lo: u32, hi: u32| {
        (
            f32_to_i32_clamped((from - 0.5).ceil()).max(lo as i32),
            f32_to_i32_clamped((to - 0.5).ceil()).min(hi as i32),
        )
    };
    let (x0, x1) = centre_span(dest.x, dest.x + dest.width, clip.x0, clip.x1);
    let (y0, y1) = centre_span(dest.y, dest.y + dest.height, clip.y0, clip.y1);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    if (src.width - dest.width).abs() < 0.001 && (src.height - dest.height).abs() < 0.001 {
        draw_1to1(
            image,
            src,
            dest,
            mode,
            alpha,
            buf,
            &mut scratch.row,
            bw,
            clip,
            x0,
            y0,
            x1,
            y1,
        );
    } else if sampling == Sampling::Nearest {
        draw_scaled(
            image, src, dest, mode, alpha, buf, bw, x0, y0, x1, y1, scratch,
        );
    } else {
        draw_filtered(
            image,
            src,
            dest,
            sampling,
            mode,
            alpha,
            buf,
            scratch,
            bw,
            clip,
            (x0, y0, x1, y1),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_1to1(
    image: &Image,
    src: Rect,
    dest: Rect,
    mode: BlendMode,
    alpha: f32,
    buf: &mut Band<'_>,
    scratch_row: &mut Vec<u32>,
    bw: u32,
    clip: ClipBox,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) {
    let max_sx = image.width as f32 - 0.001;
    let max_sy = image.height as f32 - 0.001;
    let sx0 = (x0 as f32 - dest.x) + src.x;
    let row_buf = scratch_row;
    row_buf.clear();
    row_buf.resize(usize::try_from(x1 - x0).expect("x1 > x0"), 0u32);
    for cy in y0..y1 {
        let v = (cy as f32 - dest.y) + src.y;
        let sy = f32_to_u32_clamped(v.clamp(0.0, max_sy));
        let src_row = &image.data[sy as usize * image.width as usize * 4..];
        let mut all_opaque = true;
        for (i, slot) in row_buf.iter_mut().enumerate() {
            let sx = f32_to_usize_clamped((sx0 + i as f32).clamp(0.0, max_sx));
            let ii = sx * 4;
            if ii + 3 >= src_row.len() {
                *slot = 0;
                all_opaque = false;
                continue;
            }
            let a = pixel::scale_u8(src_row[ii + 3], alpha);
            if a != 255 {
                all_opaque = false;
            }
            *slot = (u32::from(a) << 24)
                | (u32::from(src_row[ii]) << 16)
                | (u32::from(src_row[ii + 1]) << 8)
                | u32::from(src_row[ii + 2]);
        }
        if mode == BlendMode::Normal && all_opaque {
            let row_start = (cy.unsigned_abs() * bw + x0.unsigned_abs()) as usize;
            buf[row_start..row_start + row_buf.len()].copy_from_slice(row_buf.as_slice());
        } else {
            for (i, &c) in row_buf.iter().enumerate() {
                let xi = i32::try_from(i).expect("row width fits in i32");
                pixel::put(buf, bw, clip, x0 + xi, cy, c, mode);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_filtered(
    image: &Image,
    src: Rect,
    dest: Rect,
    sampling: Sampling,
    mode: BlendMode,
    alpha: f32,
    buf: &mut Band<'_>,
    scratch: &mut ImageScratch,
    bw: u32,
    clip: ClipBox,
    (x0, y0, x1, y1): (i32, i32, i32, i32),
) {
    let (iw, ih) = (
        i32::try_from(image.width).unwrap_or(i32::MAX),
        i32::try_from(image.height).unwrap_or(i32::MAX),
    );
    if iw == 0 || ih == 0 {
        return;
    }
    let bounds = sample::Bounds {
        x0: f32_to_i32_clamped(src.x.floor()).clamp(0, iw - 1),
        y0: f32_to_i32_clamped(src.y.floor()).clamp(0, ih - 1),
        x1: (f32_to_i32_clamped((src.x + src.width).ceil()) - 1).clamp(0, iw - 1),
        y1: (f32_to_i32_clamped((src.y + src.height).ceil()) - 1).clamp(0, ih - 1),
    };
    let (sx, sy) = (src.width / dest.width, src.height / dest.height);
    let shrink = sampling == Sampling::Smooth && (sx > 1.0 || sy > 1.0);
    let u0 = |cx: i32| (cx as f32 - dest.x) * sx + src.x;
    let v0 = |cy: i32| (cy as f32 - dest.y) * sy + src.y;
    let put = |buf: &mut Band<'_>, cx: i32, cy: i32, c: u32| {
        if c >> 24 != 0 {
            pixel::put(buf, bw, clip, cx, cy, c, mode);
        }
    };
    let ImageScratch {
        row: line,
        shrink: shrinking,
        taps,
        ..
    } = scratch;
    if shrink {
        shrinking.start((x0..x1).map(u0), sx, sy, bounds);
        for cy in y0..y1 {
            shrinking.row(image, (v0(cy), sy), bounds, alpha, |i, c| {
                put(buf, x0 + i32::try_from(i).unwrap_or(0), cy, c);
            });
        }
    } else if bounds.x0 <= bounds.x1 && bounds.y0 <= bounds.y1 {
        taps.clear();
        taps.extend((x0..x1).map(|cx| sample::Taps::at(u0(cx) + sx * 0.5, bounds.x0, bounds.x1)));
        line.clear();
        line.resize(taps.len(), 0);
        for cy in y0..y1 {
            let row = sample::Taps::at(v0(cy) + sy * 0.5, bounds.y0, bounds.y1);
            if sample::bilinear_row(image, taps, row, alpha, line) && mode == BlendMode::Normal {
                let start = (cy.unsigned_abs() * bw + x0.unsigned_abs()) as usize;
                buf[start..start + line.len()].copy_from_slice(line);
            } else {
                for (&c, cx) in line.iter().zip(x0..x1) {
                    put(buf, cx, cy, c);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_scaled(
    image: &Image,
    src: Rect,
    dest: Rect,
    mode: BlendMode,
    alpha: f32,
    buf: &mut Band<'_>,
    bw: u32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    scratch: &mut ImageScratch,
) {
    // The caller has checked that the pixel data fills `width * height`, and the clamps keep
    // every index inside it. The span is already inside the clip.
    let ImageScratch {
        row, source_cols, ..
    } = scratch;
    let max_sx = image.width as f32 - 0.001;
    let max_sy = image.height as f32 - 0.001;
    let stride = image.width as usize * 4;
    source_cols.clear();
    source_cols.extend((x0..x1).map(|cx| {
        let u = (cx as f32 + 0.5 - dest.x) / dest.width * src.width + src.x;
        f32_to_u32_clamped(u.clamp(0.0, max_sx)) as usize * 4
    }));
    row.clear();
    row.resize(source_cols.len(), 0);
    for cy in y0..y1 {
        let v = (cy as f32 + 0.5 - dest.y) / dest.height * src.height + src.y;
        let sy = f32_to_u32_clamped(v.clamp(0.0, max_sy)) as usize;
        let src_row = &image.data[sy * stride..(sy + 1) * stride];
        let mut all_opaque = true;
        for (slot, &ii) in row.iter_mut().zip(source_cols.iter()) {
            let a = pixel::scale_u8(src_row[ii + 3], alpha);
            all_opaque &= a == 255;
            *slot = (u32::from(a) << 24)
                | (u32::from(src_row[ii]) << 16)
                | (u32::from(src_row[ii + 1]) << 8)
                | u32::from(src_row[ii + 2]);
        }
        let start = (cy.unsigned_abs() * bw + x0.unsigned_abs()) as usize;
        if mode == BlendMode::Normal && all_opaque {
            buf[start..start + row.len()].copy_from_slice(row.as_slice());
        } else {
            for (i, &c) in row.iter().enumerate() {
                if c >> 24 != 0 {
                    let idx = start + i;
                    buf[idx] = blend_pixel(c, buf[idx], mode);
                }
            }
        }
    }
}

#[cfg(test)]
mod bad_sources {

    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;
    use crate::cpu::raster::glyph;
    use crate::cpu::raster::image::ImageScratch;
    use crate::types::{BlendMode, Color, GlyphMask, Image, Point, Rect, Sampling};

    const CLIP: ClipBox = ClipBox {
        x0: 0,
        y0: 0,
        x1: 8,
        y1: 8,
    };

    fn draw(image: &Image, src: Rect, sampling: Sampling) -> Vec<u32> {
        let mut buf = vec![0u32; 64];
        super::draw(
            image,
            src,
            Rect::new(0.0, 0.0, 8.0, 8.0),
            sampling,
            BlendMode::Normal,
            1.0,
            &mut Band::whole(&mut buf),
            &mut ImageScratch::default(),
            8,
            CLIP,
        );
        buf
    }

    #[test]
    fn empty_source_rects_and_images_draw_nothing() {
        let image = Image::new(2, 2, vec![255; 16]);
        for sampling in [Sampling::Linear, Sampling::Smooth, Sampling::Nearest] {
            for src in [
                Rect::new(0.0, 0.0, 0.0, 2.0),
                Rect::new(1.0, 1.0, -1.0, -1.0),
            ] {
                assert!(draw(&image, src, sampling).iter().all(|&p| p == 0));
            }
        }
        let empty = Image::new(0, 2, vec![255; 16]);
        let src = Rect::new(0.0, 0.0, 1.0, 1.0);
        assert!(draw(&empty, src, Sampling::Linear).iter().all(|&p| p == 0));
    }

    #[test]
    fn short_image_data_and_masks_are_skipped() {
        let short = Image::new(4, 4, vec![255; 8]);
        for sampling in [Sampling::Linear, Sampling::Smooth, Sampling::Nearest] {
            let src = Rect::new(0.0, 0.0, 2.0, 2.0);
            assert!(draw(&short, src, sampling).iter().all(|&p| p == 0));
        }
        let mask = GlyphMask::new(4, 4, vec![255; 6].into());
        let mut buf = vec![0u32; 64];
        let origin = Point::new(0.0, 0.0);
        glyph::draw(
            &mask,
            origin,
            Color::rgb(0, 0, 0),
            &mut Band::whole(&mut buf),
            8,
            CLIP,
        );
        assert!(buf.iter().all(|&p| p == 0));
    }
}

#[cfg(test)]
mod enlarged {
    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;
    use crate::cpu::raster::image::ImageScratch;
    use crate::cpu::raster::{pixel, sample};
    use crate::types::{BlendMode, Image, Rect, Sampling};

    fn pattern(w: u32, h: u32, alpha: impl Fn(u32, u32) -> u8) -> Image {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = x.to_le_bytes()[0].wrapping_mul(37) ^ y.to_le_bytes()[0].wrapping_mul(91);
                data.extend([v, v.wrapping_mul(3), 255 - v, alpha(x, y)]);
            }
        }
        Image::new(w, h, data)
    }

    /// A per-pixel bilinear read.
    fn reference(image: &Image, src: Rect, dest: Rect, mode: BlendMode, alpha: f32) -> Vec<u32> {
        let mut buf = vec![0xff20_4060; 24 * 24];
        let b = sample::Bounds {
            x0: 0,
            y0: 0,
            x1: image.width as i32 - 1,
            y1: image.height as i32 - 1,
        };
        let (sx, sy) = (src.width / dest.width, src.height / dest.height);
        for y in 0..24 {
            for x in 0..24 {
                let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                if cx < dest.x || cy < dest.y || cx >= dest.right() || cy >= dest.bottom() {
                    continue;
                }
                let u = (x as f32 - dest.x) * sx + src.x + sx * 0.5;
                let v = (y as f32 - dest.y) * sy + src.y + sy * 0.5;
                let c = sample::bilinear(image, b, u, v, alpha);
                if c >> 24 != 0 {
                    pixel::put(
                        &mut Band::whole(&mut buf),
                        24,
                        ClipBox::surface(24, 24),
                        x,
                        y,
                        c,
                        mode,
                    );
                }
            }
        }
        buf
    }

    #[test]
    fn enlarged_draws_stay_within_one_of_a_per_pixel_read() {
        let opaque = pattern(6, 5, |_, _| 255);
        let mixed = pattern(6, 5, |x, y| [255, 0, 128, 255, 7][((x + y) % 5) as usize]);
        let src = Rect::new(0.0, 0.0, 6.0, 5.0);
        let dest = Rect::new(1.3, 2.0, 21.0, 19.5);
        for image in [&opaque, &mixed] {
            for mode in [BlendMode::Normal, BlendMode::Multiply] {
                for alpha in [1.0, 0.5] {
                    let mut buf = vec![0xff20_4060; 24 * 24];
                    super::draw(
                        image,
                        src,
                        dest,
                        Sampling::Linear,
                        mode,
                        alpha,
                        &mut Band::whole(&mut buf),
                        &mut ImageScratch::default(),
                        24,
                        ClipBox::surface(24, 24),
                    );
                    let want = reference(image, src, dest, mode, alpha);
                    for (&got, &want) in buf.iter().zip(&want) {
                        let off =
                            [0, 8, 16, 24].map(|s| ((got >> s) & 255).abs_diff((want >> s) & 255));
                        assert!(
                            off.iter().all(|&d| d <= 1),
                            "{mode:?} {alpha}: {got:08x} {want:08x}"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod quads {

    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;

    use crate::cpu::raster::image::ImageScratch;
    use crate::types::{BlendMode, Image, Point, Rect, Sampling};

    fn tile() -> Image {
        let mut seed = 0x12345678u32;
        let bytes = (0..256 * 256 * 4)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed.to_le_bytes()[0]
            })
            .collect();
        Image::new(256, 256, bytes)
    }

    fn draw(
        image: &Image,
        pixels: &mut [u32],
        scratch: &mut ImageScratch,
        prepare: bool,
        q: [Point; 3],
        alpha: f32,
        mode: BlendMode,
    ) {
        super::draw_quad(
            image,
            Rect::new(2.5, 3.25, 249.0, 246.0),
            q,
            Sampling::Smooth,
            mode,
            alpha,
            &mut Band::whole(pixels),
            scratch,
            prepare,
            512,
            ClipBox::surface(512, 512).intersect(Rect::new(3.0, 2.0, 507.0, 509.0)),
        );
    }

    #[test]
    fn prepared_quad_sampling_matches_direct_reads_for_alpha_rotation_and_shrink() {
        let image = tile();
        for q in [
            [
                Point::new(5.3, 1.7),
                Point::new(500.3, 31.7),
                Point::new(-25.0, 490.7),
            ],
            [
                Point::new(100.3, 90.1),
                Point::new(270.8, 130.9),
                Point::new(60.0, 260.0),
            ],
            [
                Point::new(490.0, 20.0),
                Point::new(10.0, 41.0),
                Point::new(470.0, 505.0),
            ],
        ] {
            for alpha in [1.0, 0.6, 0.05] {
                for mode in [BlendMode::Normal, BlendMode::Multiply] {
                    let mut slow = vec![0xff123456; 512 * 512];
                    let mut fast = slow.clone();
                    draw(
                        &image,
                        &mut slow,
                        &mut ImageScratch::default(),
                        false,
                        q,
                        alpha,
                        mode,
                    );
                    draw(
                        &image,
                        &mut fast,
                        &mut ImageScratch::default(),
                        true,
                        q,
                        alpha,
                        mode,
                    );
                    assert_eq!(fast, slow, "{q:?} alpha {alpha} mode {mode:?}");
                }
            }
        }
    }
}
