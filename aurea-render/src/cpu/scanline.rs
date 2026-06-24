//! Scanline fill for paths — flat-buffer variant.

use std::cmp::Ordering;

use crate::cpu::blend::{blend_pixel, ConstSrc};
use crate::cpu::path::Edge;
use crate::numeric::f32_to_u32_clamped;
use crate::types::{BlendMode, Color};

/// Fill sorted x-crossing pairs into a scanline row (odd-even rule).
///
/// `xs` must already be sorted ascending. Called by both `fill_scanline`
/// (which gathers crossings from the full edge list) and the AET path in
/// `draw_path` (which maintains a pre-filtered active set).
#[allow(clippy::too_many_arguments)]
pub fn fill_spans(
    xs: &[f32],
    row_base: usize,
    buf: &mut [u32],
    buf_width: u32,
    offset_x: u32,
    color: Color,
    blend_mode: BlendMode,
) {
    if xs.is_empty() {
        return;
    }

    let clip_l = offset_x as f32;
    let clip_r = (offset_x + buf_width) as f32;

    let full_src = color_u32_cov(color, 1.0);
    let opaque_fast = blend_mode == BlendMode::Normal && color.a == 255;
    // For translucent Normal fills, precompute the linear source channels once
    // per span — saves 3 srgb_to_linear LUT lookups per interior pixel.
    let const_src = if !opaque_fast && blend_mode == BlendMode::Normal {
        Some(ConstSrc::new(full_src))
    } else {
        None
    };

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

    for i in (0..xs.len()).step_by(2) {
        if i + 1 >= xs.len() {
            break;
        }
        let sl = xs[i].max(clip_l);
        let sr = xs[i + 1].min(clip_r);
        if sl >= sr {
            continue;
        }

        let j0 = f32_to_u32_clamped(sl.floor());
        let j1 = f32_to_u32_clamped((sr - 0.001).floor());

        if j0 == j1 {
            write(buf, j0, sr - sl);
            continue;
        }

        write(buf, j0, (j0 as f32 + 1.0) - sl);
        write(buf, j1, sr - j1 as f32);

        fill_interior(
            buf, row_base, buf_width, offset_x, j0 + 1, j1, full_src, opaque_fast, const_src,
            blend_mode,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_interior(
    buf: &mut [u32],
    row_base: usize,
    buf_width: u32,
    offset_x: u32,
    interior_j0: u32,
    interior_j1: u32,
    full_src: u32,
    opaque_fast: bool,
    const_src: Option<ConstSrc>,
    blend_mode: BlendMode,
) {
    if interior_j0 >= interior_j1 {
        return;
    }
    let bx0 = interior_j0.saturating_sub(offset_x);
    let bx1 = interior_j1.saturating_sub(offset_x).min(buf_width);
    if bx0 >= bx1 {
        return;
    }
    let start = row_base + bx0 as usize;
    let end = row_base + bx1 as usize;
    if end > buf.len() {
        return;
    }
    if opaque_fast {
        buf[start..end].fill(full_src);
    } else if let Some(cs) = const_src {
        // Translucent Normal: use precomputed linear source.
        for p in &mut buf[start..end] {
            *p = cs.over(*p);
        }
    } else {
        for p in &mut buf[start..end] {
            *p = blend_pixel(full_src, *p, blend_mode);
        }
    }
}

/// Fill one scanline into a flat RGBA buffer (odd-even winding rule).
///
/// Scans the full edge list for crossings at `y` — O(edges) per call. For
/// single-call use in tests or simple paths; the hot path in `draw_path` uses
/// the active-edge table variant instead.
///
/// `offset_x/y` allow clipping to a sub-region (pass 0,0 for the full buffer).
/// `scratch_xs` is reused across calls to avoid a `Vec` allocation per scanline.
#[allow(clippy::too_many_arguments)]
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
    scratch_xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    let row_y = f32_to_u32_clamped(y);
    let buf_y = row_y.saturating_sub(offset_y);
    if buf_y >= buf_height {
        return;
    }
    let row_base = buf_y as usize * buf_width as usize;

    fill_spans(scratch_xs, row_base, buf, buf_width, offset_x, color, blend_mode);
}

fn color_u32_cov(c: Color, cov: f32) -> u32 {
    let a = f32_to_u32_clamped((f32::from(c.a) * cov).round());
    (a << 24) | (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
}
