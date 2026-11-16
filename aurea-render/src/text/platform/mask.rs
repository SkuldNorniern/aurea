//! Masks a shaped run is drawn into, and the ink rows of one glyph.

use super::{PlatformTextRasterizer, RunMask, SubpixelGlyph};
use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped};
use crate::text::shape::ShapedText;
use crate::types::GlyphMask;
use aurea_foundation::AureaResult;

/// The most a run's mask may take; text larger than this draws nothing.
const MAX_MASK_BYTES: usize = 1 << 28;
/// Space left around a run's ink.
const PAD: f32 = 3.0;
/// Rows fainter than this do not read as ink.
const FAINT: u8 = 64;

/// Bytes of a `width` by `height` subpixel mask, unless it is too big to make.
pub(super) fn mask_bytes(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(3)
        .filter(|&bytes| bytes <= MAX_MASK_BYTES)
}

/// `shaped` drawn into an RGB subpixel mask.
pub(super) fn draw_run(
    rasterizer: &dyn PlatformTextRasterizer,
    shaped: &ShapedText,
) -> AureaResult<RunMask> {
    let ascent = if shaped.vertical {
        0.0
    } else {
        shaped.ink_ascent.max(0.0)
    };
    let (w, h) = if shaped.vertical {
        (shaped.size, shaped.width)
    } else {
        (shaped.width, ascent + shaped.ink_descent.max(0.0))
    };
    let dev_w = f32_to_u32_clamped((w + PAD * 2.0).ceil().max(1.0));
    let dev_h = f32_to_u32_clamped((h + PAD * 2.0).ceil().max(1.0));
    let Some(bytes) = mask_bytes(dev_w, dev_h) else {
        return Ok((GlyphMask::new(0, 0, Vec::new().into()), ascent, PAD));
    };
    let mut coverage = vec![0u8; bytes];

    let baseline = (ascent + PAD).round();
    let centre = PAD + shaped.size / 2.0;
    for (pen, glyph) in shaped.pen_positions() {
        let g = rasterizer.rasterize_subpixel(glyph.face, glyph.id, shaped.size)?;
        if g.width == 0 || g.height == 0 {
            continue;
        }
        let (x, y) = if shaped.vertical {
            (centre + glyph.offset_x, PAD + pen + glyph.offset_y)
        } else {
            (PAD + pen + glyph.offset_x, baseline + glyph.offset_y)
        };
        let left = f32_to_i32_clamped(x.round()).saturating_add(g.left);
        let top = f32_to_i32_clamped(y.round()).saturating_add(g.top);
        blit_max(&mut coverage, (dev_w, dev_h), &g, (left, top));
    }

    Ok((GlyphMask::new(dev_w, dev_h, coverage.into()), ascent, PAD))
}

/// Max-blends `g` into `coverage`, a `dev` sized RGB mask, with its top left at `origin`.
fn blit_max(
    coverage: &mut [u8],
    (dev_w, dev_h): (u32, u32),
    g: &SubpixelGlyph,
    (left, top): (i32, i32),
) {
    let (dev_w, dev_h) = (dev_w as usize, dev_h as usize);
    for row in 0..g.height {
        let Ok(dy) = usize::try_from(i64::from(top) + i64::from(row)) else {
            continue;
        };
        if dy >= dev_h {
            continue;
        }
        for col in 0..g.width {
            let Ok(dx) = usize::try_from(i64::from(left) + i64::from(col)) else {
                continue;
            };
            if dx >= dev_w {
                continue;
            }
            let src = ((row * g.width + col) * 3) as usize;
            let dst = (dy * dev_w + dx) * 3;
            for (d, s) in coverage[dst..dst + 3]
                .iter_mut()
                .zip(&g.coverage[src..src + 3])
            {
                *d = (*d).max(*s);
            }
        }
    }
}

/// The first row with ink, counted from the glyph's top, and how many rows down to the last.
/// `None` for a glyph with no ink or no width.
pub(super) fn ink_rows(coverage: &[u8], width: u32) -> Option<(i32, u32)> {
    let row_bytes = width as usize * 3;
    if row_bytes == 0 {
        return None;
    }
    let (mut first, mut last) = (None, None);
    for (i, line) in coverage.chunks(row_bytes).enumerate() {
        if line.iter().any(|&c| c >= FAINT) {
            first.get_or_insert(i);
            last = Some(i);
        }
    }
    let first = i32::try_from(first?).ok()?;
    let last = i32::try_from(last?).ok()?;
    Some((first, u32::try_from(last - first + 1).ok()?))
}
