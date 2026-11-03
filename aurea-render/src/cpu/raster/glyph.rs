//! Glyph masks: per-channel coverage, mixed in linear light.

use crate::cpu::blend::{ConstSrc, LinearLuts, blend_pixel};
use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::cpu::raster::pixel;
use crate::numeric::f32_to_i32_clamped;
use crate::types::{BlendMode, Color, GlyphMask, Point};

#[cfg(test)]
pub(crate) fn draw(
    mask: &GlyphMask,
    origin: Point,
    color: Color,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
) {
    draw_blended(mask, origin, color, BlendMode::Normal, buf, bw, clip);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_blended(
    mask: &GlyphMask,
    origin: Point,
    color: Color,
    mode: BlendMode,
    buf: &mut Band<'_>,
    bw: u32,
    clip: ClipBox,
) {
    if mask.width == 0
        || mask.height == 0
        || color.a == 0
        || !pixel::bytes_fit(mask.width, mask.height, 3, mask.coverage.len())
    {
        return;
    }
    let luts = LinearLuts::get();
    let (tr, tg, tb) = (
        luts.linear(color.r),
        luts.linear(color.g),
        luts.linear(color.b),
    );
    let opaque_pixel =
        0xFF00_0000 | (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b);
    let source = ConstSrc::new(opaque_pixel);
    let dx = f32_to_i32_clamped(origin.x.round());
    let dy = f32_to_i32_clamped(origin.y.round());

    let offset = |edge: u32, origin: i32, limit: u32| {
        usize::try_from((i64::from(edge) - i64::from(origin)).clamp(0, i64::from(limit)))
            .unwrap_or(0)
    };
    let x_lo = offset(clip.x0, dx, mask.width);
    let x_hi = offset(clip.x1, dx, mask.width);
    let y_lo = offset(clip.y0, dy, mask.height);
    let y_hi = offset(clip.y1, dy, mask.height);
    if x_lo >= x_hi || y_lo >= y_hi {
        return;
    }

    let px = usize::try_from(i64::from(dx) + x_lo as i64).unwrap_or(0);
    let py = usize::try_from(i64::from(dy) + y_lo as i64).unwrap_or(0);
    // Colour alpha applied to each coverage byte. At 255 that is the identity, so no table.
    let fade_table = (color.a != 255).then(|| {
        let alpha = u16::from(color.a);
        let mut fade = [0u8; 256];
        for (c, slot) in fade.iter_mut().enumerate() {
            let c = u16::try_from(c).unwrap_or(0);
            *slot = u8::try_from((c * alpha + 127) / 255).unwrap_or(255);
        }
        fade
    });
    let fade = |c: u8| fade_table.as_ref().map_or(c, |t| t[usize::from(c)]);
    let normal = mode == BlendMode::Normal;
    let rgb = opaque_pixel & 0x00ff_ffff;
    for my in y_lo..y_hi {
        let row = my * mask.width as usize;
        let cov_row = &mask.coverage[(row + x_lo) * 3..(row + x_hi) * 3];
        let start = (py + my - y_lo) * bw as usize + px;
        let dst_row = &mut buf[start..start + x_hi - x_lo];

        for (coverage, dst) in cov_row.as_chunks::<3>().0.iter().zip(dst_row) {
            let cr8 = fade(coverage[0]);
            let cg8 = fade(coverage[1]);
            let cb8 = fade(coverage[2]);
            if cr8 == 0 && cg8 == 0 && cb8 == 0 {
                continue;
            }

            if normal && cr8 == 255 && cg8 == 255 && cb8 == 255 {
                *dst = opaque_pixel;
                continue;
            }
            if !normal || (*dst >> 24) != 255 {
                let coverage = u32::from(cr8.max(cg8).max(cb8));
                *dst = if normal {
                    source.over_at_alpha(*dst, coverage)
                } else {
                    blend_pixel((coverage << 24) | rgb, *dst, mode)
                };
                continue;
            }
            let da = (*dst >> 24) & 0xff;
            let [_, dr, dg, db] = dst.to_be_bytes();
            let or_ = luts.mix(tr, dr, cr8);
            let og = luts.mix(tg, dg, cg8);
            let ob = luts.mix(tb, db, cb8);
            let sa = u32::from(cr8.max(cg8).max(cb8));
            let oa = sa + ((255 - sa) * da) / 255;
            *dst = (oa << 24) | (or_ << 16) | (og << 8) | ob;
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::cpu::blend::{ConstSrc, LinearLuts, blend_pixel};
    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;
    use crate::cpu::raster::fill::PathScratch;
    use crate::cpu::raster::image::ImageScratch;
    use crate::cpu::raster::{item, pixel, rect};
    use crate::numeric::f32_to_i32_clamped;
    use crate::scene::command::DrawCommand;
    use crate::scene::display_list::{CacheKey, DisplayIndex, DisplayItem};
    use crate::types::{BlendMode, Color, GlyphMask, Point, Rect};

    #[test]
    fn clear_under_a_clip_off_the_surface_draws_nothing() {
        let mut pixels = vec![7u32; 100];
        let clip = ClipBox::surface(10, 10).intersect(Rect::new(40.0, 40.0, 5.0, 5.0));
        let whole = Rect::new(0.0, 0.0, 10.0, 10.0);
        rect::clear(&whole, 0, &mut Band::whole(&mut pixels), 10, clip);
        assert!(pixels.iter().all(|&p| p == 7));
    }

    #[test]
    fn glyph_color_alpha_scales_coverage() {
        let mask = GlyphMask::new(1, 1, vec![255; 3].into());
        for alpha in [0, 64, 128, 255] {
            let mut pixels = vec![0u32; 1];
            super::draw(
                &mask,
                Point::new(0.0, 0.0),
                Color::rgba(255, 255, 255, alpha),
                &mut Band::whole(&mut pixels),
                1,
                ClipBox::surface(1, 1),
            );
            assert_eq!(pixels[0] >> 24, u32::from(alpha));
        }
    }

    #[test]
    fn glyph_item_opacity_combines_with_color_alpha() {
        let item = DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(1),
            Rect::new(0.0, 0.0, 1.0, 1.0),
            false,
            BlendMode::Normal,
            DrawCommand::DrawGlyphMask(
                GlyphMask::new(1, 1, vec![255; 3].into()),
                Point::new(0.0, 0.0),
                Color::rgba(255, 255, 255, 128),
            ),
        )
        .with_opacity(0.5);
        let mut pixels = [0];
        item::render(
            &item,
            &mut Band::whole(&mut pixels),
            &mut PathScratch::default(),
            &mut ImageScratch::default(),
            1,
            ClipBox::surface(1, 1),
        )
        .expect("render");
        assert_eq!(pixels[0] >> 24, 64);
    }

    #[test]
    fn glyph_blends_respect_transparency_and_multiply() {
        let mask = GlyphMask::new(1, 1, vec![128, 64, 32].into());
        let mut transparent = [0];
        super::draw(
            &mask,
            Point::new(0.0, 0.0),
            Color::rgb(255, 0, 0),
            &mut Band::whole(&mut transparent),
            1,
            ClipBox::surface(1, 1),
        );
        assert_eq!(
            transparent[0], 0x80ff_0000,
            "straight RGB on a transparent layer"
        );
        let mask = GlyphMask::new(1, 1, vec![255; 3].into());
        let mut background = [0xff12_3456];
        super::draw_blended(
            &mask,
            Point::new(0.0, 0.0),
            Color::rgb(255, 255, 255),
            BlendMode::Multiply,
            &mut Band::whole(&mut background),
            1,
            ClipBox::surface(1, 1),
        );
        assert_eq!(
            background[0], 0xff12_3456,
            "white multiply preserves its backdrop"
        );
    }

    #[test]
    fn clipped_glyph_pixels_match_full_render() {
        let data: Vec<_> = (0..12 * 16 * 3)
            .map(|i| u8::try_from((i * 73) % 256).expect("byte"))
            .collect();
        let mask = GlyphMask::new(12, 16, data.into());
        for origin in [
            Point::new(0.0, 0.0),
            Point::new(-4.0, -7.0),
            Point::new(8.0, 10.0),
        ] {
            for alpha in [64, 255] {
                let mut full = vec![0xff12_3456; 20 * 20];
                let mut clipped = full.clone();
                let clip = ClipBox::surface(20, 20).intersect(Rect::new(3.0, 4.0, 11.0, 9.0));
                let color = Color::rgba(240, 120, 60, alpha);
                super::draw(
                    &mask,
                    origin,
                    color,
                    &mut Band::whole(&mut full),
                    20,
                    ClipBox::surface(20, 20),
                );
                super::draw(
                    &mask,
                    origin,
                    color,
                    &mut Band::whole(&mut clipped),
                    20,
                    clip,
                );
                for y in 0..20 {
                    for x in 0..20 {
                        let expected = if clip.contains(x, y) {
                            full[usize::try_from(y * 20 + x).expect("pixel index")]
                        } else {
                            0xff12_3456
                        };
                        assert_eq!(
                            clipped[usize::try_from(y * 20 + x).expect("pixel index")],
                            expected
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn far_offscreen_glyphs_do_not_overflow_clip_arithmetic() {
        let mask = GlyphMask::new(1, 1, vec![255; 3].into());
        for origin in [
            Point::new(f32::MIN, 0.0),
            Point::new(0.0, f32::MIN),
            Point::new(f32::MAX, f32::MAX),
        ] {
            let mut pixels = vec![0u32; 4];
            super::draw(
                &mask,
                origin,
                Color::rgb(255, 255, 255),
                &mut Band::whole(&mut pixels),
                2,
                ClipBox::surface(2, 2),
            );
            assert_eq!(pixels, [0; 4]);
        }
    }

    /// The per-pixel glyph blend before the fade table; the oracle for `draw_glyph_blended`.
    #[allow(clippy::too_many_arguments)]
    fn reference_glyph_blended(
        mask: &GlyphMask,
        origin: Point,
        color: Color,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        if mask.width == 0
            || mask.height == 0
            || color.a == 0
            || !pixel::bytes_fit(mask.width, mask.height, 3, mask.coverage.len())
        {
            return;
        }
        let luts = LinearLuts::get();
        let (tr, tg, tb) = (
            luts.linear(color.r),
            luts.linear(color.g),
            luts.linear(color.b),
        );
        let opaque_pixel = 0xFF00_0000
            | (u32::from(color.r) << 16)
            | (u32::from(color.g) << 8)
            | u32::from(color.b);
        let source = ConstSrc::new(opaque_pixel);
        let dx = f32_to_i32_clamped(origin.x.round());
        let dy = f32_to_i32_clamped(origin.y.round());

        let offset = |edge: u32, origin: i32, limit: u32| {
            usize::try_from((i64::from(edge) - i64::from(origin)).clamp(0, i64::from(limit)))
                .unwrap_or(0)
        };
        let x_lo = offset(clip.x0, dx, mask.width);
        let x_hi = offset(clip.x1, dx, mask.width);
        let y_lo = offset(clip.y0, dy, mask.height);
        let y_hi = offset(clip.y1, dy, mask.height);
        if x_lo >= x_hi || y_lo >= y_hi {
            return;
        }

        let px = usize::try_from(i64::from(dx) + x_lo as i64).unwrap_or(0);
        let py = usize::try_from(i64::from(dy) + y_lo as i64).unwrap_or(0);
        let alpha = u16::from(color.a);
        let fade =
            |coverage: u8| u8::try_from((u16::from(coverage) * alpha + 127) / 255).unwrap_or(255);
        for my in y_lo..y_hi {
            let row = my * mask.width as usize;
            let cov_row = &mask.coverage[(row + x_lo) * 3..(row + x_hi) * 3];
            let start = (py + my - y_lo) * bw as usize + px;
            let dst_row = &mut buf[start..start + x_hi - x_lo];

            for (coverage, dst) in cov_row.as_chunks::<3>().0.iter().zip(dst_row) {
                let (cr8, cg8, cb8) = if color.a == 255 {
                    (coverage[0], coverage[1], coverage[2])
                } else {
                    (fade(coverage[0]), fade(coverage[1]), fade(coverage[2]))
                };
                if cr8 == 0 && cg8 == 0 && cb8 == 0 {
                    continue;
                }

                if mode == BlendMode::Normal && cr8 == 255 && cg8 == 255 && cb8 == 255 {
                    *dst = opaque_pixel;
                    continue;
                }
                if mode != BlendMode::Normal || (*dst >> 24) != 255 {
                    let coverage = u32::from(cr8.max(cg8).max(cb8));
                    *dst = if mode == BlendMode::Normal {
                        source.over_at_alpha(*dst, coverage)
                    } else {
                        blend_pixel((coverage << 24) | (opaque_pixel & 0x00ff_ffff), *dst, mode)
                    };
                    continue;
                }
                let da = (*dst >> 24) & 0xff;
                let [_, dr, dg, db] = dst.to_be_bytes();
                let or_ = luts.mix(tr, dr, cr8);
                let og = luts.mix(tg, dg, cg8);
                let ob = luts.mix(tb, db, cb8);
                let sa = u32::from(cr8.max(cg8).max(cb8));
                let oa = sa + ((255 - sa) * da) / 255;
                *dst = (oa << 24) | (or_ << 16) | (og << 8) | ob;
            }
        }
    }

    #[test]
    fn glyph_blend_matches_per_pixel_reference() {
        let (gw, gh) = (37u32, 19u32);
        let coverage: Vec<u8> = (0..gw * gh * 3)
            .map(|i| u8::try_from((i * 73) % 256).expect("byte"))
            .collect();
        let mask = GlyphMask::new(gw, gh, coverage.into());
        let (sw, sh) = (40u32, 24u32);
        let clip = ClipBox::surface(sw, sh);
        let backdrop: Vec<u32> = (0..sw * sh)
            .map(|i| {
                let v = i.wrapping_mul(2_654_435_761);
                match i % 4 {
                    0 => 0,
                    1 => 0x8000_0000 | (v & 0x00ff_ffff),
                    2 => 0xff00_0000 | (v & 0x00ff_ffff),
                    _ => v,
                }
            })
            .collect();
        for alpha in [0, 1, 64, 128, 254, 255] {
            for color in [
                Color::rgba(200, 90, 17, alpha),
                Color::rgba(255, 255, 255, alpha),
            ] {
                for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
                    for origin in [Point::new(2.0, 3.0), Point::new(-5.0, 20.0)] {
                        let mut expected = backdrop.clone();
                        reference_glyph_blended(
                            &mask,
                            origin,
                            color,
                            mode,
                            &mut expected,
                            sw,
                            clip,
                        );
                        let mut actual = backdrop.clone();
                        super::draw_blended(
                            &mask,
                            origin,
                            color,
                            mode,
                            &mut Band::whole(&mut actual),
                            sw,
                            clip,
                        );
                        for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
                            assert_eq!(a, e, "alpha {alpha} {mode:?} {origin:?} pixel {i}");
                        }
                    }
                }
            }
        }
    }
}
