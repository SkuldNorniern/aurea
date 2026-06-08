//! Scanline fill for paths — flat-buffer variant.

use super::super::types::{BlendMode, Color};
use super::blend::blend_pixel;
use super::path::Edge;

/// Fill one scanline into a flat RGBA buffer (odd-even winding rule).
/// `offset_x/y` allow clipping to a sub-region (pass 0,0 for the full buffer).
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
) {
    let mut xs: Vec<f32> = edges
        .iter()
        .filter(|e| y >= e.y_min && y < e.y_max)
        .map(|e| e.x_at_y(y))
        .collect();

    if xs.is_empty() {
        return;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let row_y = y as u32;
    let buf_y = row_y.saturating_sub(offset_y);
    if buf_y >= buf_height {
        return;
    }

    let clip_l = offset_x as f32;
    let clip_r = (offset_x + buf_width) as f32;

    for i in (0..xs.len()).step_by(2) {
        if i + 1 >= xs.len() {
            break;
        }
        let sl = xs[i].max(clip_l);
        let sr = xs[i + 1].min(clip_r);
        if sl >= sr {
            continue;
        }

        let j0 = sl.floor() as u32;
        let j1 = (sr - 0.001).floor() as u32;

        for j in j0..=j1 {
            let jf = j as f32;
            let cov = ((jf + 1.0).min(sr) - jf.max(sl)).max(0.0);
            if cov <= 0.0 {
                continue;
            }

            let bx = j.saturating_sub(offset_x);
            if bx >= buf_width {
                continue;
            }

            let idx = buf_y as usize * buf_width as usize + bx as usize;
            if idx >= buf.len() {
                continue;
            }

            let c = color_u32_cov(color, cov);
            buf[idx] = blend_pixel(c, buf[idx], blend_mode);
        }
    }
}

fn color_u32_cov(c: Color, cov: f32) -> u32 {
    let a = (c.a as f32 * cov).round().clamp(0.0, 255.0) as u32;
    (a << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}
