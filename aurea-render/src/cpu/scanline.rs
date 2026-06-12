//! Scanline fill for paths — flat-buffer variant.

use super::super::types::{BlendMode, Color};
use super::blend::blend_pixel;
use super::path::Edge;

/// Fill one scanline into a flat RGBA buffer (odd-even winding rule).
/// `offset_x/y` allow clipping to a sub-region (pass 0,0 for the full buffer).
/// `scratch_xs` is reused across calls to avoid a `Vec` allocation per scanline.
pub fn fill_scanline(
    edges: &[Edge],
    y: f32,
    buf: &mut [u32],
    buf_width: u32,
    buf_height: u32,
    offset_x: u32,
    offset_y: u32,
    color: Color,
    blend_mode: BlendMode,
    scratch_xs: &mut Vec<f32>,
) {
    scratch_xs.clear();
    scratch_xs.extend(
        edges
            .iter()
            .filter(|e| y >= e.y_min && y < e.y_max)
            .map(|e| e.x_at_y(y)),
    );

    if scratch_xs.is_empty() {
        return;
    }
    scratch_xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let row_y = y as u32;
    let buf_y = row_y.saturating_sub(offset_y);
    if buf_y >= buf_height {
        return;
    }
    let row_base = buf_y as usize * buf_width as usize;

    let clip_l = offset_x as f32;
    let clip_r = (offset_x + buf_width) as f32;

    // Fully-covered pixels (the common case for the interior of a filled
    // span) skip the per-pixel coverage math entirely; opaque+Normal spans
    // go through `fill()` instead of a per-pixel blend.
    let opaque_fast = blend_mode == BlendMode::Normal && color.a == 255;
    let full_src = color_u32_cov(color, 1.0);

    let write = |buf: &mut [u32], j: u32, cov: f32| {
        if cov <= 0.0 {
            return;
        }
        let bx = j.saturating_sub(offset_x);
        if bx >= buf_width {
            return;
        }
        let idx = row_base + bx as usize;
        if idx >= buf.len() {
            return;
        }
        let src = if cov >= 1.0 {
            full_src
        } else {
            color_u32_cov(color, cov)
        };
        buf[idx] = blend_pixel(src, buf[idx], blend_mode);
    };

    for i in (0..scratch_xs.len()).step_by(2) {
        if i + 1 >= scratch_xs.len() {
            break;
        }
        let sl = scratch_xs[i].max(clip_l);
        let sr = scratch_xs[i + 1].min(clip_r);
        if sl >= sr {
            continue;
        }

        let j0 = sl.floor() as u32;
        let j1 = (sr - 0.001).floor() as u32;

        if j0 == j1 {
            write(buf, j0, sr - sl);
            continue;
        }

        write(buf, j0, (j0 as f32 + 1.0) - sl);
        write(buf, j1, sr - j1 as f32);

        let interior_j0 = j0 + 1;
        let interior_j1 = j1;
        if interior_j0 < interior_j1 {
            let bx0 = interior_j0.saturating_sub(offset_x);
            let bx1 = interior_j1.saturating_sub(offset_x).min(buf_width);
            if bx0 < bx1 {
                let start = row_base + bx0 as usize;
                let end = row_base + bx1 as usize;
                if end <= buf.len() {
                    if opaque_fast {
                        buf[start..end].fill(full_src);
                    } else {
                        for idx in start..end {
                            buf[idx] = blend_pixel(full_src, buf[idx], blend_mode);
                        }
                    }
                }
            }
        }
    }
}

fn color_u32_cov(c: Color, cov: f32) -> u32 {
    let a = (c.a as f32 * cov).round().clamp(0.0, 255.0) as u32;
    (a << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}
