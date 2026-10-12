//! The tile grid partial repaint works in: which tiles a region covers, which are dirty, and
//! clearing them.

use crate::numeric::f32_to_u32_clamped;
use crate::types::Rect;

use super::damage::union_rect;

pub(super) const TILE_SIZE: u32 = 128;

pub(super) fn union_dirty_tile_rects(
    dirty_tiles: &[bool],
    tiles_x: u32,
    tiles_y: u32,
    bw: u32,
    bh: u32,
) -> Option<Rect> {
    let mut acc: Option<Rect> = None;
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            if dirty_tiles[(ty * tiles_x + tx) as usize] {
                let tr = tile_rect(tx, ty, bw, bh);
                acc = Some(match acc {
                    None => tr,
                    Some(a) => union_rect(a, tr),
                });
            }
        }
    }
    acc
}

pub(super) fn tile_grid_dims(bw: u32, bh: u32) -> (u32, u32) {
    (bw.div_ceil(TILE_SIZE), bh.div_ceil(TILE_SIZE))
}

pub(super) fn tile_range(bounds: Rect, tiles_x: u32, tiles_y: u32) -> (u32, u32, u32, u32) {
    let tile = TILE_SIZE as f32;
    let x0 = f32_to_u32_clamped((bounds.x / tile).floor().max(0.0)).min(tiles_x);
    let y0 = f32_to_u32_clamped((bounds.y / tile).floor().max(0.0)).min(tiles_y);
    let x1 = f32_to_u32_clamped(((bounds.x + bounds.width) / tile).ceil().max(0.0))
        .min(tiles_x)
        .max(x0);
    let y1 = f32_to_u32_clamped(((bounds.y + bounds.height) / tile).ceil().max(0.0))
        .min(tiles_y)
        .max(y0);
    (x0, y0, x1, y1)
}

pub(super) fn tile_rect(tx: u32, ty: u32, bw: u32, bh: u32) -> Rect {
    let x0 = (tx * TILE_SIZE) as f32;
    let y0 = (ty * TILE_SIZE) as f32;
    let x1 = ((tx + 1) * TILE_SIZE).min(bw) as f32;
    let y1 = ((ty + 1) * TILE_SIZE).min(bh) as f32;
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

pub(super) fn item_overlaps_dirty_tiles(
    bounds: Rect,
    dirty: &[bool],
    tiles_x: u32,
    tiles_y: u32,
) -> bool {
    let (tx0, ty0, tx1, ty1) = tile_range(bounds, tiles_x, tiles_y);
    (ty0..ty1).any(|ty| (tx0..tx1).any(|tx| dirty[(ty * tiles_x + tx) as usize]))
}

pub(super) fn mark_tile_range_dirty(rect: Rect, tiles_x: u32, tiles_y: u32, dirty: &mut [bool]) {
    let (tx0, ty0, tx1, ty1) = tile_range(rect, tiles_x, tiles_y);
    for ty in ty0..ty1 {
        for tx in tx0..tx1 {
            dirty[(ty * tiles_x + tx) as usize] = true;
        }
    }
}

pub(super) fn round_out_clamp(r: Rect, bw: u32, bh: u32) -> Rect {
    let x0 = r.x.floor().max(0.0);
    let y0 = r.y.floor().max(0.0);
    let x1 = (r.x + r.width).ceil().min(bw as f32);
    let y1 = (r.y + r.height).ceil().min(bh as f32);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}
