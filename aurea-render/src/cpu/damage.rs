//! What a frame has to repaint: the damage a new display list causes, and
//! which items in it can be skipped because something opaque covers them.

use std::f32::consts::FRAC_1_SQRT_2;

use crate::scene::command::DrawCommand;
use crate::scene::display_list::DisplayItem;
use crate::types::{BlendMode, Point, Rect};

use super::tiles::{item_overlaps_dirty_tiles, round_out_clamp, tile_range, tile_rect};

/// Result of diffing the current display list against the previous frame's.
#[derive(Debug)]
pub(super) enum FrameDamage {
    /// The list is positionally identical (same cache keys) to last frame.
    Unchanged,
    /// An item with unknown bounds changed; repaint everything.
    Full,
    /// Only this region changed.
    Region(Rect),
}

/// Bounds are "known" if non-empty; `compute_bounds` returns `(0,0,0,0)` for
/// commands it can't size (e.g. clip/transform/opacity push-pop markers).
pub(super) fn is_known_bounds(r: Rect) -> bool {
    r.width > 0.0 && r.height > 0.0
}

/// Resolves explicit (`forced`) and diffed (`diff`) damage into the rect that
/// must be repainted this frame. `Some(None)` means "repaint everything";
/// `None` means the frame is unchanged and rendering can be skipped entirely.
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

/// Whether `items[i]` needs rendering: it must have known bounds that
/// overlap a dirty tile, and must not be fully hidden by later draws.
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

/// Marks each item that later opaque draws completely cover.
///
/// Asking "does any later item hide this one?" per item reads the rest of the
/// list every time, which is quadratic. Measured on a scene of 8000 rects:
///
/// ```text
/// per frame     scan      no occlusion    tile sweep
/// 1000 items    550us     240us           124us
/// 2000 items   1893us     467us           244us
/// 4000 items   7162us    1015us           490us
/// 8000 items  26820us    1404us           836us
/// ```
///
/// The scan cost more than the drawing it saved — 19 times the frame with the
/// check removed at 8000 items. The sweep is linear and beats both.
///
/// This walks the list backwards instead, keeping a tile per bit of what is
/// already painted over. An item is hidden when every tile it touches is
/// covered, and an opaque item covers the tiles that sit entirely inside it.
/// The cost is a tile sweep per item rather than a list scan.
///
/// Tiles also let several draws combine to hide something that no single one
/// of them contains, which the old check could not see.
///
/// It errs towards drawing: a tile only counts as covered when a draw plainly
/// contains it, so an item is never skipped over a partially covered tile.
pub(super) fn mark_occluded(
    items: &[DisplayItem],
    (tiles_x, tiles_y, bw, bh): (u32, u32, u32, u32),
    covered: &mut Vec<bool>,
    occluded: &mut Vec<bool>,
) {
    let tile_count = (tiles_x * tiles_y) as usize;
    covered.clear();
    covered.resize(tile_count, false);
    occluded.clear();
    occluded.resize(items.len(), false);
    if tile_count == 0 {
        return;
    }

    for (i, item) in items.iter().enumerate().rev() {
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

        // Then let this item hide what comes before it.
        if !item.opaque || item.blend_mode != BlendMode::Normal {
            continue;
        }
        let Some(region) = covered_region(item) else {
            continue;
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
}

/// Smallest rect covering both `a` and `b`.
pub(super) fn union_rect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.width).max(b.x + b.width);
    let y1 = (a.y + a.height).max(b.y + b.height);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Whether `outer` fully contains `inner`.
pub(super) fn rect_contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// The area a draw actually covers: the part of its shape that is solid,
/// less anything its clip cuts away.
///
/// Bounds are not that area for every shape. A circle's bounds are the square
/// around it, and the corners of that square are not painted — so a circle
/// wide enough to swallow a tile could hide something the tile's corner still
/// showed. What is used instead is the largest square that fits inside it.
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

/// The largest axis-aligned square inside a circle.
pub(super) fn inscribed_square(center: Point, radius: f32) -> Rect {
    // Half the side of a square inscribed in a circle of radius r is r/sqrt(2).
    let half = radius * FRAC_1_SQRT_2;
    Rect::new(center.x - half, center.y - half, half * 2.0, half * 2.0)
}

/// The overlap of `a` and `b`, or `None` when they do not meet.
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
