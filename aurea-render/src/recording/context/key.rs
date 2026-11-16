//! Cache keys for display items: what a command paints, and the state it paints under.

use super::State;
use crate::effect::Effect;
use crate::scene::command::DrawCommand;
use crate::scene::display_list::CacheKey;
use crate::types::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// The key of `command` recorded under `state` at `scale`.
pub(super) fn command_key(command: &DrawCommand, state: &State, scale: f32) -> CacheKey {
    let mut hasher = DefaultHasher::new();
    hash_command(command, &mut hasher);
    state.opacity.to_bits().hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    state.blend_mode.hash(&mut hasher);
    match state.clip_rect {
        Some(clip) => {
            1u8.hash(&mut hasher);
            hash_rect(clip, &mut hasher);
        }
        None => 0u8.hash(&mut hasher),
    }
    CacheKey::from_hash(hasher.finish())
}

/// `key` made apart from items under other clip paths, which `clip_hash` identifies.
pub(super) fn under_clip(key: CacheKey, clip_hash: u64) -> CacheKey {
    let mut hasher = DefaultHasher::new();
    key.0.hash(&mut hasher);
    clip_hash.hash(&mut hasher);
    CacheKey(hasher.finish())
}

pub(super) fn clip_paths_hash(paths: &[Path]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for path in paths {
        path.commands.len().hash(&mut hasher);
        hash_path(path, &mut hasher);
    }
    hasher.finish()
}

fn hash_command(command: &DrawCommand, h: &mut impl Hasher) {
    match command {
        DrawCommand::Clear(color) => {
            "Clear".hash(h);
            color.hash(h);
        }
        DrawCommand::DrawRect(rect, paint) => {
            "DrawRect".hash(h);
            hash_rect(*rect, h);
            hash_paint(paint, h);
        }
        DrawCommand::DrawRoundRect(rect, radius, paint) => {
            "DrawRoundRect".hash(h);
            hash_rect(*rect, h);
            radius.to_bits().hash(h);
            hash_paint(paint, h);
        }
        DrawCommand::DrawCircle(center, radius, paint) => {
            "DrawCircle".hash(h);
            hash_point(*center, h);
            radius.to_bits().hash(h);
            hash_paint(paint, h);
        }
        DrawCommand::DrawShadow(rect, radius, blur, color) => {
            "DrawShadow".hash(h);
            hash_rect(*rect, h);
            radius.to_bits().hash(h);
            blur.to_bits().hash(h);
            color.hash(h);
        }
        DrawCommand::Backdrop(rect, effects) => hash_backdrop(*rect, effects, h),
        DrawCommand::DrawImageRect(image, dest, sampling) => {
            "DrawImageRect".hash(h);
            sampling.hash(h);
            image.width.hash(h);
            image.height.hash(h);
            hash_rect(*dest, h);
            image.id().hash(h);
        }
        DrawCommand::DrawImageRegion(image, src, dest, sampling) => {
            "DrawImageRegion".hash(h);
            sampling.hash(h);
            image.width.hash(h);
            image.height.hash(h);
            hash_rect(*src, h);
            hash_rect(*dest, h);
            image.id().hash(h);
        }
        DrawCommand::DrawImageQuad(image, src, quad, sampling) => {
            "DrawImageQuad".hash(h);
            image.id().hash(h);
            sampling.hash(h);
            hash_rect(*src, h);
            hash_quad(quad, h);
        }
        DrawCommand::DrawTexture(texture, src, dest, sampling) => {
            "DrawTexture".hash(h);
            texture.hash(h);
            sampling.hash(h);
            hash_rect(*src, h);
            hash_rect(*dest, h);
        }
        DrawCommand::DrawTextureQuad(texture, src, quad, sampling) => {
            "DrawTextureQuad".hash(h);
            texture.hash(h);
            sampling.hash(h);
            hash_rect(*src, h);
            hash_quad(quad, h);
        }
        DrawCommand::FillLinearGradient(grad, rect) => {
            "FillLinearGradient".hash(h);
            hash_point(grad.start, h);
            hash_point(grad.end, h);
            hash_rect(*rect, h);
            hash_stops(&grad.stops, h);
        }
        DrawCommand::FillRadialGradient(grad, rect) => {
            "FillRadialGradient".hash(h);
            hash_point(grad.center, h);
            grad.radius.to_bits().hash(h);
            hash_rect(*rect, h);
            hash_stops(&grad.stops, h);
        }
        DrawCommand::DrawColumns(x, wide, spans, color) => {
            "DrawColumns".hash(h);
            x.to_bits().hash(h);
            wide.to_bits().hash(h);
            for (top, bottom) in spans {
                top.to_bits().hash(h);
                bottom.to_bits().hash(h);
            }
            color.hash(h);
        }
        DrawCommand::DrawPath(path, paint) => {
            "DrawPath".hash(h);
            hash_path(path, h);
            hash_paint(paint, h);
        }
        DrawCommand::DrawGlyphMask(mask, origin, color) => {
            "DrawGlyphMask".hash(h);
            mask.id().hash(h);
            mask.width.hash(h);
            mask.height.hash(h);
            hash_point(*origin, h);
            color.hash(h);
        }
    }
}

fn hash_rect(r: Rect, h: &mut impl Hasher) {
    for v in [r.x, r.y, r.width, r.height] {
        v.to_bits().hash(h);
    }
}

fn hash_point(p: Point, h: &mut impl Hasher) {
    p.x.to_bits().hash(h);
    p.y.to_bits().hash(h);
}

fn hash_quad(quad: &[Point; 3], h: &mut impl Hasher) {
    for p in quad {
        hash_point(*p, h);
    }
}

fn hash_stops(stops: &[GradientStop], h: &mut impl Hasher) {
    for stop in stops {
        stop.offset.to_bits().hash(h);
        stop.color.hash(h);
    }
}

/// Everything a stroke or fill paints with, so no two styles share a key.
fn hash_paint(paint: &Paint, h: &mut impl Hasher) {
    paint.color.hash(h);
    paint.style.hash(h);
    paint.stroke_width.to_bits().hash(h);
    paint.line_cap.hash(h);
    paint.line_join.hash(h);
    paint.miter_limit.to_bits().hash(h);
    match &paint.dash {
        Some(dash) => {
            1u8.hash(h);
            dash.offset.to_bits().hash(h);
            (dash.anchored, dash.along_edges).hash(h);
            for length in dash.lengths() {
                length.to_bits().hash(h);
            }
        }
        None => 0u8.hash(h),
    }
}

/// The commands of `path` and its fill rule, which decides what it fills.
fn hash_path(path: &Path, h: &mut impl Hasher) {
    path.fill_rule.hash(h);
    for cmd in &path.commands {
        match cmd {
            PathCommand::MoveTo(p) => {
                0u8.hash(h);
                hash_point(*p, h);
            }
            PathCommand::LineTo(p) => {
                1u8.hash(h);
                hash_point(*p, h);
            }
            PathCommand::QuadTo(c, p) => {
                2u8.hash(h);
                hash_point(*c, h);
                hash_point(*p, h);
            }
            PathCommand::CubicTo(c1, c2, p) => {
                3u8.hash(h);
                hash_point(*c1, h);
                hash_point(*c2, h);
                hash_point(*p, h);
            }
            PathCommand::Close => 4u8.hash(h),
        }
    }
}

/// A backdrop's box and effects.
fn hash_backdrop(rect: Rect, effects: &[Effect], h: &mut impl Hasher) {
    "Backdrop".hash(h);
    hash_rect(rect, h);
    for effect in effects {
        format!("{effect:?}").hash(h);
    }
}
