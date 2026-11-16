//! Geometry the recorder snaps, bounds and clips with, in physical pixels unless noted.

use crate::scene::command::{DrawCommand, quad_bounds};
use crate::scene::hit_test::path_bounds;
use crate::types::*;
use std::f32::consts::SQRT_2;

/// The rectangle a path traces, when it is one axis-aligned rectangle.
pub(super) fn axis_aligned_rect(path: &Path) -> Option<Rect> {
    let mut points = [Point::default(); 5];
    let mut n = 0;
    for command in &path.commands {
        match command {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => {
                *points.get_mut(n)? = *p;
                n += 1;
            }
            PathCommand::Close => {}
            PathCommand::QuadTo(..) | PathCommand::CubicTo(..) => return None,
        }
    }
    if n == 5 && near(points[0], points[4]) {
        n = 4;
    }
    if n != 4 {
        return None;
    }

    let horizontal_first = near_value(points[0].y, points[1].y)
        && near_value(points[1].x, points[2].x)
        && near_value(points[2].y, points[3].y)
        && near_value(points[3].x, points[0].x);
    let vertical_first = near_value(points[0].x, points[1].x)
        && near_value(points[1].y, points[2].y)
        && near_value(points[2].x, points[3].x)
        && near_value(points[3].y, points[0].y);
    if !horizontal_first && !vertical_first {
        return None;
    }

    let xs = [points[0].x, points[1].x, points[2].x, points[3].x];
    let ys = [points[0].y, points[1].y, points[2].y, points[3].y];
    let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
}

fn near(a: Point, b: Point) -> bool {
    near_value(a.x, b.x) && near_value(a.y, b.y)
}

fn near_value(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// The corners of `r`, clockwise from its top left.
pub(super) fn corners(r: Rect) -> [Point; 4] {
    [
        Point::new(r.x, r.y),
        Point::new(r.x + r.width, r.y),
        Point::new(r.x + r.width, r.y + r.height),
        Point::new(r.x, r.y + r.height),
    ]
}

/// A closed four-sided path through `corners`.
pub(super) fn quad_path(corners: [Point; 4]) -> Path {
    let mut path = Path::new();
    path.commands.push(PathCommand::MoveTo(corners[0]));
    for corner in &corners[1..] {
        path.commands.push(PathCommand::LineTo(*corner));
    }
    path.commands.push(PathCommand::Close);
    path
}

pub(super) fn intersect_rects(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// How far a stroke with `paint` reaches past its path.
pub(super) fn stroke_reach(paint: &Paint) -> f32 {
    let half = paint.stroke_width / 2.0;
    let miter = if paint.line_join == LineJoin::Miter {
        half * paint.miter_limit.max(1.0)
    } else {
        half
    };
    let cap = if paint.line_cap == LineCap::Square {
        half * SQRT_2
    } else {
        half
    };
    miter.max(cap)
}

/// `rect` grown by `reach` on every side.
fn outset(rect: Rect, reach: f32) -> Rect {
    Rect::new(
        rect.x - reach,
        rect.y - reach,
        rect.width + 2.0 * reach,
        rect.height + 2.0 * reach,
    )
}

/// `rect` grown by half the stroke width when `paint` strokes.
fn stroke_bounds(rect: Rect, paint: &Paint) -> Rect {
    if paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0 {
        outset(rect, paint.stroke_width / 2.0)
    } else {
        rect
    }
}

/// A rectangle's fill or stroke, snapped to whole pixels. Returns the rect and the stroke width.
pub(super) fn snap_rect(rect: Rect, paint: &Paint) -> (Rect, f32) {
    let stroke = paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0;
    let width = if stroke {
        paint.stroke_width.round().max(1.0)
    } else {
        paint.stroke_width
    };
    let inset = if stroke { width / 2.0 } else { 0.0 };
    let (x, w) = snap_span(rect.x, rect.width, inset);
    let (y, h) = snap_span(rect.y, rect.height, inset);
    (Rect::new(x, y, w, h), width)
}

/// One axis of a snapped fill or stroke: its edges round on their own, `inset` in from each.
fn snap_span(start: f32, len: f32, inset: f32) -> (f32, f32) {
    if len <= 0.0 {
        return (start.round(), 0.0);
    }
    let outer = (start - inset).round();
    let end = (start + len + inset).round();
    let size = (end - outer).max(1.0 + 2.0 * inset);
    (outer + inset, size - 2.0 * inset)
}

/// A straight rule snapped: its ends round on their own like a fill's edges, its thickness rounds
/// as a whole so every rule of one width comes out alike.
pub(super) fn snap_rule(rect: Rect, horizontal: bool) -> Rect {
    let ((x, w), (y, h)) = if horizontal {
        (edges(rect.x, rect.width), keep(rect.y, rect.height))
    } else {
        (keep(rect.x, rect.width), edges(rect.y, rect.height))
    };
    Rect::new(x, y, w, h)
}

/// Thickness rounded to whole pixels, centred on the original.
fn keep(start: f32, len: f32) -> (f32, f32) {
    let size = len.round().max(1.0);
    ((start + (len - size) / 2.0).round(), size)
}

/// Both ends rounded to whole pixels.
fn edges(start: f32, len: f32) -> (f32, f32) {
    let outer = start.round();
    (outer, ((start + len).round() - outer).max(1.0))
}

/// The box a recorded command can paint, for damage and hit tests.
pub(super) fn command_bounds(command: &DrawCommand) -> Rect {
    match command {
        DrawCommand::Clear(_) => Rect::new(0.0, 0.0, f32::MAX, f32::MAX),
        DrawCommand::DrawRect(rect, paint) => stroke_bounds(*rect, paint),
        DrawCommand::DrawRoundRect(rect, _, paint) => stroke_bounds(*rect, paint),
        DrawCommand::DrawCircle(center, radius, paint) => stroke_bounds(
            Rect::new(
                center.x - radius,
                center.y - radius,
                radius * 2.0,
                radius * 2.0,
            ),
            paint,
        ),
        DrawCommand::Backdrop(rect, _) => *rect,
        DrawCommand::DrawShadow(rect, _, blur, _) => outset(*rect, blur * 3.0),
        DrawCommand::DrawColumns(x, wide, spans, _) => {
            let (mut top, mut bottom) = (f32::MAX, f32::MIN);
            for &(t, b) in spans {
                top = top.min(t);
                bottom = bottom.max(b);
            }
            Rect::new(*x, top, *wide * spans.len() as f32, (bottom - top).max(0.0))
        }
        DrawCommand::DrawImageRect(_, dest, _)
        | DrawCommand::DrawImageRegion(_, _, dest, _)
        | DrawCommand::DrawTexture(_, _, dest, _) => *dest,
        DrawCommand::DrawImageQuad(_, _, quad, _) | DrawCommand::DrawTextureQuad(_, _, quad, _) => {
            quad_bounds(quad)
        }
        DrawCommand::DrawGlyphMask(mask, origin, _) => {
            Rect::new(origin.x, origin.y, mask.width as f32, mask.height as f32)
        }
        DrawCommand::FillLinearGradient(_, rect) | DrawCommand::FillRadialGradient(_, rect) => {
            *rect
        }
        DrawCommand::DrawPath(path, paint) => {
            let bounds = path_bounds(path);
            if paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0 {
                outset(bounds, stroke_reach(paint))
            } else {
                bounds
            }
        }
    }
}
