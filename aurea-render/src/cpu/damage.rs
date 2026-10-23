//! What a frame has to repaint: the damage a new display list causes, and which items in it can be
//! skipped because something opaque covers them.

use std::f32::consts::FRAC_1_SQRT_2;

use crate::scene::command::DrawCommand;
use crate::scene::display_list::DisplayItem;
use crate::types::{BlendMode, Point, Rect};

use super::tiles::{item_overlaps_dirty_tiles, round_out_clamp, tile_range, tile_rect};

#[derive(Debug)]
pub(super) enum FrameDamage {
    /// The list is positionally identical (same cache keys) to last frame.
    Unchanged,
    /// An item with unknown bounds changed; repaint everything.
    Full,
    /// Only this region changed.
    Region(Rect),
}

pub(super) fn is_known_bounds(r: Rect) -> bool {
    r.width > 0.0 && r.height > 0.0
}

pub(super) fn resolve_frame_damage(
    forced: Option<Rect>,
    diff: FrameDamage,
    bw: u32,
    bh: u32,
) -> Option<Option<Rect>> {
    match (forced, diff) {
        (None, FrameDamage::Unchanged) => None,
        (None, FrameDamage::Full) | (Some(_), FrameDamage::Full) => Some(None),
        (None, FrameDamage::Region(r)) => Some(Some(round_out_clamp(r, bw, bh))),
        (Some(p), FrameDamage::Unchanged) => Some(Some(p)),
        (Some(p), FrameDamage::Region(r)) => Some(Some(round_out_clamp(union_rect(p, r), bw, bh))),
    }
}

pub(super) fn should_render_item(
    item: &DisplayItem,
    occluded: &[bool],
    i: usize,
    dirty_tiles: &[bool],
    tiles_x: u32,
    tiles_y: u32,
) -> bool {
    if !is_known_bounds(item.bounds) {
        return true;
    }
    item_overlaps_dirty_tiles(item.bounds, dirty_tiles, tiles_x, tiles_y) && !occluded[i]
}

/// Marks the items opaque draws after them hide. Returns the last clear, with the tiles the draws
/// after it cover in `under_clear`.
pub(super) fn mark_occluded(
    items: &[DisplayItem],
    (tiles_x, tiles_y, bw, bh): (u32, u32, u32, u32),
    covered: &mut Vec<bool>,
    occluded: &mut Vec<bool>,
    under_clear: &mut Vec<bool>,
) -> Option<usize> {
    let tile_count = (tiles_x * tiles_y) as usize;
    covered.clear();
    covered.resize(tile_count, false);
    occluded.clear();
    occluded.resize(items.len(), false);
    if tile_count == 0 {
        return None;
    }

    let mut last_clear = None;
    for (i, item) in items.iter().enumerate().rev() {
        if last_clear.is_none() && matches!(item.command, DrawCommand::Clear(_)) {
            last_clear = Some(i);
            under_clear.clone_from(covered);
        }
        if !is_known_bounds(item.bounds) {
            continue;
        }
        let (tx0, ty0, tx1, ty1) = tile_range(item.bounds, tiles_x, tiles_y);
        if tx0 == tx1 || ty0 == ty1 {
            continue;
        }

        occluded[i] = (ty0..ty1)
            .flat_map(|ty| (tx0..tx1).map(move |tx| (tx, ty)))
            .all(|(tx, ty)| covered[(ty * tiles_x + tx) as usize]);

        cover(item, (tiles_x, tiles_y, bw, bh), covered);
    }
    last_clear
}

/// Marks the tiles an opaque `item` paints over entirely.
fn cover(
    item: &DisplayItem,
    (tiles_x, tiles_y, bw, bh): (u32, u32, u32, u32),
    covered: &mut [bool],
) {
    if !item.opaque || item.blend_mode != BlendMode::Normal {
        return;
    }
    let Some(region) = covered_region(item) else {
        return;
    };
    let (cx0, cy0, cx1, cy1) = tile_range(region, tiles_x, tiles_y);
    for ty in cy0..cy1 {
        for tx in cx0..cx1 {
            if rect_contains(region, tile_rect(tx, ty, bw, bh)) {
                covered[(ty * tiles_x + tx) as usize] = true;
            }
        }
    }
}

/// Each run of tiles along a tile row that `covered` does not mark, as one rect.
pub(super) fn uncovered_runs(
    covered: &[bool],
    (tiles_x, tiles_y, bw, bh): (u32, u32, u32, u32),
    mut run: impl FnMut(Rect),
) {
    for ty in 0..tiles_y {
        let mut tx = 0;
        while tx < tiles_x {
            if covered[(ty * tiles_x + tx) as usize] {
                tx += 1;
                continue;
            }
            let first = tile_rect(tx, ty, bw, bh);
            while tx < tiles_x && !covered[(ty * tiles_x + tx) as usize] {
                tx += 1;
            }
            let last = tile_rect(tx - 1, ty, bw, bh);
            run(Rect::new(
                first.x,
                first.y,
                last.right() - first.x,
                first.height,
            ));
        }
    }
}

pub(super) fn union_rect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.width).max(b.x + b.width);
    let y1 = (a.y + a.height).max(b.y + b.height);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

pub(super) fn rect_contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

pub(super) fn covered_region(item: &DisplayItem) -> Option<Rect> {
    let solid = match &item.command {
        DrawCommand::DrawCircle(center, radius, _) => inscribed_square(*center, *radius),
        _ => item.bounds,
    };
    match item.clip {
        None => Some(solid),
        Some(clip) => intersect_rect(solid, clip),
    }
}

pub(super) fn inscribed_square(center: Point, radius: f32) -> Rect {
    let half = radius * FRAC_1_SQRT_2;
    Rect::new(center.x - half, center.y - half, half * 2.0, half * 2.0)
}

pub(super) fn intersect_rect(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
}
