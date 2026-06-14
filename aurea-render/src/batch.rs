//! Backend-agnostic 2D batches lowered from a [`DisplayList`].
//!
//! The GPU painters (ZenGPU, wgpu) consume `RenderBatches` instead of walking
//! the display list themselves, so the rect-batching / instance-layout logic
//! lives in exactly one place and both backends draw identical geometry.
//!
//! Scope is G4 "Rung 1": background `Clear` plus solid-colour, axis-aligned
//! `DrawRect` fills. Circles, images, gradients, and text are lowered in later
//! rungs; commands this rung doesn't understand are skipped (the CPU rasterizer
//! remains the fallback for full fidelity until those land).

use super::command::DrawCommand;
use super::display_list::DisplayList;
use super::types::{Color, PaintStyle};

/// One solid-colour rectangle, ready to upload as a GPU instance.
///
/// `rect` is `[x, y, width, height]` in **physical** (HiDPI-scaled) pixels —
/// the same space the swapchain extent is in — and `color` is straight
/// (non-premultiplied) RGBA in `0.0..=1.0`. The painter is responsible for any
/// premultiply / blend-state setup. `#[repr(C)]` so the struct can be uploaded
/// directly as a per-instance vertex attribute (8 contiguous `f32`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct RectInstance {
    /// `[x, y, width, height]` in physical pixels.
    pub rect: [f32; 4],
    /// Straight RGBA, each channel in `0.0..=1.0`.
    pub color: [f32; 4],
}

impl RectInstance {
    fn from_rect(rect: super::types::Rect, color: Color) -> Self {
        Self {
            rect: [rect.x, rect.y, rect.width, rect.height],
            color: [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ],
        }
    }
}

/// A single frame's 2D draw work, lowered from a display list and independent
/// of any GPU backend.
#[derive(Debug, Clone, Default)]
pub struct RenderBatches {
    /// Colour the frame opened with, if it began (or was reset) by a `Clear`.
    /// `None` means "don't clear" — the painter loads the previous contents.
    pub clear: Option<Color>,
    /// Solid-colour rectangles in submission (painter's-algorithm) order.
    pub rects: Vec<RectInstance>,
}

impl RenderBatches {
    /// Lower a display list into backend-agnostic batches.
    ///
    /// Commands are walked in order so the painter can reproduce the CPU
    /// rasterizer's semantics with a back-to-front draw. A `Clear` matches the
    /// rasterizer by covering the whole frame, so it both records the clear
    /// colour and discards any rects already collected this frame.
    pub fn lower(list: &DisplayList) -> Self {
        let mut batches = RenderBatches::default();
        for item in list.items() {
            match &item.command {
                DrawCommand::Clear(color) => {
                    batches.clear = Some(*color);
                    batches.rects.clear();
                }
                DrawCommand::DrawRect(rect, paint) if paint.style == PaintStyle::Fill => {
                    batches.rects.push(RectInstance::from_rect(*rect, paint.color));
                }
                // Other commands (strokes, circles, images, gradients, text)
                // are lowered in later rungs.
                _ => {}
            }
        }
        batches
    }

    /// True when there's nothing to clear and nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.clear.is_none() && self.rects.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::DrawCommand;
    use crate::display_list::{CacheKey, DisplayItem, NodeId};
    use crate::types::{BlendMode, Paint, Rect};

    fn item(command: DrawCommand) -> DisplayItem {
        DisplayItem::new(
            NodeId(0),
            CacheKey::from_hash(0),
            Rect::new(0.0, 0.0, 0.0, 0.0),
            false,
            BlendMode::Normal,
            command,
        )
    }

    #[test]
    fn clear_sets_color() {
        let mut list = DisplayList::new();
        list.push(item(DrawCommand::Clear(Color::rgb(10, 20, 30))));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.clear, Some(Color::rgb(10, 20, 30)));
        assert!(b.rects.is_empty());
    }

    #[test]
    fn fill_rect_is_collected() {
        let mut list = DisplayList::new();
        let paint = Paint::new().color(Color::rgb(255, 0, 0));
        list.push(item(DrawCommand::DrawRect(Rect::new(1.0, 2.0, 3.0, 4.0), paint)));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.rects.len(), 1);
        assert_eq!(b.rects[0].rect, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(b.rects[0].color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn stroke_rect_is_skipped() {
        let mut list = DisplayList::new();
        let paint = Paint::new().style(PaintStyle::Stroke);
        list.push(item(DrawCommand::DrawRect(Rect::new(0.0, 0.0, 8.0, 8.0), paint)));
        let b = RenderBatches::lower(&list);
        assert!(b.rects.is_empty());
    }

    #[test]
    fn clear_after_rects_wipes_them() {
        let mut list = DisplayList::new();
        let paint = Paint::new().color(Color::rgb(0, 255, 0));
        list.push(item(DrawCommand::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), paint)));
        list.push(item(DrawCommand::Clear(Color::rgb(0, 0, 0))));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.clear, Some(Color::rgb(0, 0, 0)));
        assert!(b.rects.is_empty(), "clear must discard rects drawn before it");
    }

    #[test]
    fn rects_after_clear_survive() {
        let mut list = DisplayList::new();
        let paint = Paint::new().color(Color::rgb(0, 0, 255));
        list.push(item(DrawCommand::Clear(Color::rgb(0, 0, 0))));
        list.push(item(DrawCommand::DrawRect(Rect::new(5.0, 5.0, 2.0, 2.0), paint)));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.clear, Some(Color::rgb(0, 0, 0)));
        assert_eq!(b.rects.len(), 1);
    }
}
