//! Scanline fill for paths.
//!
//! Walks horizontal scanlines, finds where edges cross, and fills between
//! pairs of crossings (odd-even rule), writing into a tile with blending.

use super::super::types::{BlendMode, Color};
use super::blend::blend_pixel;
use super::path::Edge;

/// Fills one scanline: finds edge crossings, sorts by x, fills between pairs, clamped to the tile.
pub fn fill_scanline(
    edges: &[Edge],
    y: f32,
    tile_pixels: &mut [u32],
    tile_width: u32,
    tile_height: u32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    color: Color,
    blend_mode: BlendMode,
) {
    let mut active_edges: Vec<f32> = edges
        .iter()
        .filter(|e| y >= e.y_min && y < e.y_max)
        .map(|e| e.x_at_y(y))
        .collect();

    if active_edges.is_empty() {
        return;
    }

    active_edges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    for i in (0..active_edges.len()).step_by(2) {
        if i + 1 >= active_edges.len() {
            break;
        }

        let x_start_f = active_edges[i].max(0.0);
        let x_end_f = active_edges[i + 1].max(0.0);

        if x_start_f >= x_end_f {
            continue;
        }

        let tile_y = (y as u32).saturating_sub(tile_offset_y);
        if tile_y >= tile_height {
            continue;
        }

        let clip_left = tile_offset_x as f32;
        let clip_right = (tile_offset_x + tile_width) as f32;
        let span_left = x_start_f.max(clip_left);
        let span_right = x_end_f.min(clip_right);

        if span_left >= span_right {
            continue;
        }

        let j_start = span_left.floor() as u32;
        let j_end = (span_right - 0.001).floor() as u32;

        for j in j_start..=j_end {
            let jf = j as f32;
            let overlap_left = jf.max(span_left);
            let overlap_right = (jf + 1.0).min(span_right);
            let coverage = (overlap_right - overlap_left).max(0.0);

            if coverage <= 0.0 {
                continue;
            }

            let tile_x = j.saturating_sub(tile_offset_x);
            if tile_x < tile_width {
                let idx = (tile_y as usize) * (tile_width as usize) + (tile_x as usize);
                if idx < tile_pixels.len() {
                    let color_u32 = color_to_u32_with_coverage(color, coverage);
                    let dst = tile_pixels[idx];
                    tile_pixels[idx] = blend_pixel(color_u32, dst, blend_mode);
                }
            }
        }
    }
}

fn color_to_u32_with_coverage(color: Color, coverage: f32) -> u32 {
    let a = ((color.a as f32 * coverage).round().clamp(0.0, 255.0)) as u32;
    (a << 24) | ((color.r as u32) << 16) | ((color.g as u32) << 8) | (color.b as u32)
}
