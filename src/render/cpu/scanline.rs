//! Scanline fill algorithm for path rendering
//!
//! Fills paths using the scanline algorithm with edge lists

use super::super::types::Color;
use super::path::Edge;

/// Fill a scanline region using active edge list
pub fn fill_scanline(
    edges: &[Edge],
    y: f32,
    tile_pixels: &mut [u32],
    tile_width: u32,
    tile_height: u32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    color: Color,
) {
    // Find active edges at this scanline
    let mut active_edges: Vec<f32> = edges
        .iter()
        .filter(|e| y >= e.y_min && y < e.y_max)
        .map(|e| e.x_at_y(y))
        .collect();

    if active_edges.is_empty() {
        return;
    }

    // Sort x coordinates
    active_edges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Fill between pairs of edges (odd-even rule)
    for i in (0..active_edges.len()).step_by(2) {
        if i + 1 >= active_edges.len() {
            break;
        }

        let x_start = active_edges[i].max(0.0) as u32;
        let x_end = active_edges[i + 1].max(0.0) as u32;

        // Clamp to tile bounds
        let x_start = x_start.max(tile_offset_x).min(tile_offset_x + tile_width);
        let x_end = x_end.max(tile_offset_x).min(tile_offset_x + tile_width);

        if x_start >= x_end {
            continue;
        }

        // Check if this scanline is within tile bounds
        let tile_y = (y as u32).saturating_sub(tile_offset_y);
        if tile_y >= tile_height {
            continue;
        }

        // Fill pixels
        let color_u32 = color_to_u32(color);
        for x in x_start..x_end {
            let tile_x = x.saturating_sub(tile_offset_x);
            if tile_x < tile_width {
                let idx = (tile_y as usize) * (tile_width as usize) + (tile_x as usize);
                if idx < tile_pixels.len() {
                    tile_pixels[idx] = color_u32;
                }
            }
        }
    }
}

/// Convert color to u32 (RGBA)
fn color_to_u32(color: Color) -> u32 {
    ((color.a as u32) << 24) | ((color.r as u32) << 16) | ((color.g as u32) << 8) | (color.b as u32)
}
