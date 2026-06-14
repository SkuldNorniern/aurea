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
use super::types::{
    Color, GradientStop, Image, LinearGradient, PaintStyle, Point, RadialGradient, Rect,
};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Weak};

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

/// One solid-colour filled circle, ready to upload as a GPU instance.
///
/// `center_radius` is `[cx, cy, radius, _]` in **physical** pixels; `color` is
/// straight RGBA in `0.0..=1.0`. Same 32-byte `#[repr(C)]` layout as
/// [`RectInstance`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct CircleInstance {
    /// `[cx, cy, radius, _padding]` in physical pixels.
    pub center_radius: [f32; 4],
    /// Straight RGBA, each channel in `0.0..=1.0`.
    pub color: [f32; 4],
}

impl CircleInstance {
    fn new(center: Point, radius: f32, color: Color) -> Self {
        Self {
            center_radius: [center.x, center.y, radius, 0.0],
            color: color_f32(color),
        }
    }
}

/// One gradient fill over a rect. `a[3]` is the kind flag: `0.0` linear,
/// `1.0` radial. `lut` is a 256x1 tightly packed RGBA8 lookup texture.
///
/// - **Linear:** `a = [start.x, start.y, _, 0.0]`, `b = [end.x, end.y, _, _]`.
/// - **Radial:** `a = [center.x, center.y, radius, 1.0]`, `b` unused.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientInstance {
    /// Fill area `[x, y, w, h]` in physical pixels.
    pub rect: [f32; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub lut: Arc<[u8]>,
}

impl GradientInstance {
    fn linear(rect: Rect, grad: &LinearGradient, lut: Arc<[u8]>) -> Self {
        Self {
            rect: [rect.x, rect.y, rect.width, rect.height],
            a: [grad.start.x, grad.start.y, 0.0, 0.0],
            b: [grad.end.x, grad.end.y, 0.0, 0.0],
            lut,
        }
    }

    fn radial(rect: Rect, grad: &RadialGradient, lut: Arc<[u8]>) -> Self {
        Self {
            rect: [rect.x, rect.y, rect.width, rect.height],
            a: [grad.center.x, grad.center.y, grad.radius, 1.0],
            b: [0.0, 0.0, 0.0, 0.0],
            lut,
        }
    }
}

fn color_f32(c: Color) -> [f32; 4] {
    [
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    ]
}

/// Stable in-process content key for retaining LUT allocations across frames.
fn gradient_lut_key(stops: &[GradientStop]) -> u64 {
    let mut hasher = DefaultHasher::new();
    stops.len().hash(&mut hasher);
    for stop in stops {
        stop.offset.to_bits().hash(&mut hasher);
        [stop.color.r, stop.color.g, stop.color.b, stop.color.a].hash(&mut hasher);
    }
    hasher.finish()
}

fn gradient_color_at(stops: &[GradientStop], t: f32) -> Color {
    if stops.is_empty() {
        return Color::rgba(0, 0, 0, 0);
    }
    if stops.len() == 1 {
        return stops[0].color;
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0].offset, pair[1].offset);
        if t >= a && t <= b {
            let s = if (b - a).abs() < 1e-6 {
                1.0
            } else {
                (t - a) / (b - a)
            };
            let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * s).round() as u8;
            let (c0, c1) = (pair[0].color, pair[1].color);
            return Color::rgba(
                lerp(c0.r, c1.r),
                lerp(c0.g, c1.g),
                lerp(c0.b, c1.b),
                lerp(c0.a, c1.a),
            );
        }
    }
    if t <= stops[0].offset {
        stops[0].color
    } else {
        stops.last().expect("non-empty gradient stops").color
    }
}

fn build_gradient_lut(stops: &[GradientStop]) -> Arc<[u8]> {
    let mut lut = Vec::with_capacity(256 * 4);
    for i in 0..256 {
        let color = gradient_color_at(stops, i as f32 / 255.0);
        lut.extend_from_slice(&[color.r, color.g, color.b, color.a]);
    }
    lut.into()
}

/// An image to blit: `image` is the (Arc-backed) pixel source, `dest` the
/// destination rect in physical pixels, `src` the source sub-rect in image
/// pixels, `tint` a straight-RGBA multiply. The GPU texture is resolved by the
/// backend (which owns the device) — the batch layer stays device-agnostic and
/// just carries the `Image`.
#[derive(Debug, Clone)]
pub struct ImageDraw {
    pub image: Image,
    pub dest: Rect,
    pub src: Rect,
    pub tint: Color,
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
    /// LUT-sampled gradient fills in submission order.
    pub gradients: Vec<GradientInstance>,
    /// Images to blit in submission order.
    pub images: Vec<ImageDraw>,
    /// Solid-colour filled circles in submission order.
    pub circles: Vec<CircleInstance>,
    gradient_lut_cache: HashMap<u64, Weak<[u8]>>,
}

impl RenderBatches {
    /// Lower a display list into freshly-allocated batches.
    ///
    /// Prefer [`RenderBatches::lower_into`] in a render loop to reuse the
    /// allocation across frames.
    pub fn lower(list: &DisplayList) -> Self {
        let mut batches = RenderBatches::default();
        batches.lower_into(list);
        batches
    }

    /// Clear and refill from `list`, **reusing** the existing `rects`
    /// allocation. This is the per-frame hot path: a renderer keeps one
    /// `RenderBatches` and calls this each frame, so steady-state framing does
    /// no heap allocation once the buffer has grown to its working size.
    ///
    /// Commands are walked in order so the painter can reproduce the CPU
    /// rasterizer's semantics with a back-to-front draw. A `Clear` matches the
    /// rasterizer by covering the whole frame, so it both records the clear
    /// colour and discards any rects already collected this frame.
    pub fn lower_into(&mut self, list: &DisplayList) {
        self.clear = None;
        self.rects.clear();
        self.gradients.clear();
        self.images.clear();
        self.circles.clear();
        for item in list.items() {
            match &item.command {
                DrawCommand::Clear(color) => {
                    self.clear = Some(*color);
                    self.rects.clear();
                    self.gradients.clear();
                    self.images.clear();
                    self.circles.clear();
                }
                DrawCommand::DrawRect(rect, paint) if paint.style == PaintStyle::Fill => {
                    self.rects.push(RectInstance::from_rect(*rect, paint.color));
                }
                DrawCommand::DrawCircle(center, radius, paint)
                    if paint.style == PaintStyle::Fill =>
                {
                    self.circles
                        .push(CircleInstance::new(*center, *radius, paint.color));
                }
                DrawCommand::FillLinearGradient(grad, rect) => {
                    let lut = self.gradient_lut(&grad.stops);
                    self.gradients
                        .push(GradientInstance::linear(*rect, grad, lut));
                }
                DrawCommand::FillRadialGradient(grad, rect) => {
                    let lut = self.gradient_lut(&grad.stops);
                    self.gradients
                        .push(GradientInstance::radial(*rect, grad, lut));
                }
                DrawCommand::DrawImageRect(image, dest) => {
                    self.images.push(ImageDraw {
                        image: image.clone(),
                        dest: *dest,
                        src: Rect::new(0.0, 0.0, image.width as f32, image.height as f32),
                        tint: Color::rgb(255, 255, 255),
                    });
                }
                DrawCommand::DrawImageRegion(image, src, dest) => {
                    self.images.push(ImageDraw {
                        image: image.clone(),
                        dest: *dest,
                        src: *src,
                        tint: Color::rgb(255, 255, 255),
                    });
                }
                // Other commands (strokes, glyph masks, text) are lowered later.
                _ => {}
            }
        }
    }

    /// True when there's nothing to clear and nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.clear.is_none()
            && self.rects.is_empty()
            && self.gradients.is_empty()
            && self.images.is_empty()
            && self.circles.is_empty()
    }

    fn gradient_lut(&mut self, stops: &[GradientStop]) -> Arc<[u8]> {
        let key = gradient_lut_key(stops);
        if let Some(lut) = self.gradient_lut_cache.get(&key).and_then(Weak::upgrade) {
            return lut;
        }
        let lut = build_gradient_lut(stops);
        self.gradient_lut_cache.insert(key, Arc::downgrade(&lut));
        lut
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
        list.push(item(DrawCommand::DrawRect(
            Rect::new(1.0, 2.0, 3.0, 4.0),
            paint,
        )));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.rects.len(), 1);
        assert_eq!(b.rects[0].rect, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(b.rects[0].color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn stroke_rect_is_skipped() {
        let mut list = DisplayList::new();
        let paint = Paint::new().style(PaintStyle::Stroke);
        list.push(item(DrawCommand::DrawRect(
            Rect::new(0.0, 0.0, 8.0, 8.0),
            paint,
        )));
        let b = RenderBatches::lower(&list);
        assert!(b.rects.is_empty());
    }

    #[test]
    fn fill_circle_is_collected() {
        use crate::types::Point;
        let mut list = DisplayList::new();
        let paint = Paint::new().color(Color::rgb(0, 128, 255));
        list.push(item(DrawCommand::DrawCircle(
            Point::new(10.0, 20.0),
            5.0,
            paint,
        )));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.circles.len(), 1);
        assert_eq!(b.circles[0].center_radius, [10.0, 20.0, 5.0, 0.0]);
        assert!(b.rects.is_empty());
    }

    #[test]
    fn stroke_circle_is_skipped() {
        use crate::types::Point;
        let mut list = DisplayList::new();
        let paint = Paint::new().style(PaintStyle::Stroke);
        list.push(item(DrawCommand::DrawCircle(
            Point::new(0.0, 0.0),
            8.0,
            paint,
        )));
        let b = RenderBatches::lower(&list);
        assert!(b.circles.is_empty());
    }

    #[test]
    fn linear_gradient_is_collected() {
        use crate::types::{GradientStop, LinearGradient};
        let mut list = DisplayList::new();
        let grad = LinearGradient {
            start: Point::new(0.0, 0.0),
            end: Point::new(100.0, 0.0),
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: Color::rgb(255, 0, 0),
                },
                GradientStop {
                    offset: 1.0,
                    color: Color::rgb(0, 0, 255),
                },
            ],
        };
        list.push(item(DrawCommand::FillLinearGradient(
            grad,
            Rect::new(0.0, 0.0, 100.0, 50.0),
        )));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.gradients.len(), 1);
        assert_eq!(b.gradients[0].a[3], 0.0, "linear kind flag");
        assert_eq!(&b.gradients[0].lut[..4], &[255, 0, 0, 255]);
        assert_eq!(&b.gradients[0].lut[1020..], &[0, 0, 255, 255]);
    }

    #[test]
    fn radial_gradient_carries_radius_and_kind() {
        use crate::types::{GradientStop, RadialGradient};
        let mut list = DisplayList::new();
        let grad = RadialGradient {
            center: Point::new(50.0, 50.0),
            radius: 25.0,
            stops: vec![GradientStop {
                offset: 0.0,
                color: Color::rgb(10, 20, 30),
            }],
        };
        list.push(item(DrawCommand::FillRadialGradient(
            grad,
            Rect::new(0.0, 0.0, 100.0, 100.0),
        )));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.gradients.len(), 1);
        assert_eq!(b.gradients[0].a[2], 25.0, "radius");
        assert_eq!(b.gradients[0].a[3], 1.0, "radial kind flag");
    }

    #[test]
    fn multi_stop_gradient_lut_preserves_middle_stop() {
        let lut = build_gradient_lut(&[
            GradientStop {
                offset: 0.0,
                color: Color::rgb(255, 0, 0),
            },
            GradientStop {
                offset: 0.5,
                color: Color::rgb(0, 255, 0),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgb(0, 0, 255),
            },
        ]);
        let middle = 128 * 4;
        assert!(lut[middle] < 5);
        assert!(lut[middle + 1] > 250);
        assert!(lut[middle + 2] < 5);
        assert_eq!(lut[middle + 3], 255);
    }

    #[test]
    fn repeated_lowering_reuses_gradient_lut_arc() {
        let mut list = DisplayList::new();
        list.push(item(DrawCommand::FillLinearGradient(
            LinearGradient {
                start: Point::new(0.0, 0.0),
                end: Point::new(10.0, 0.0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: Color::rgb(0, 0, 0),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::rgb(255, 255, 255),
                    },
                ],
            },
            Rect::new(0.0, 0.0, 10.0, 10.0),
        )));
        let mut batches = RenderBatches::default();
        batches.lower_into(&list);
        let first = Arc::clone(&batches.gradients[0].lut);
        batches.lower_into(&list);
        assert!(Arc::ptr_eq(&first, &batches.gradients[0].lut));
    }

    #[test]
    fn full_image_is_collected() {
        let mut list = DisplayList::new();
        let image = Image::new(2, 3, vec![255; 24]);
        list.push(item(DrawCommand::DrawImageRect(
            image,
            Rect::new(10.0, 20.0, 30.0, 40.0),
        )));

        let b = RenderBatches::lower(&list);
        assert_eq!(b.images.len(), 1);
        assert_eq!(b.images[0].dest, Rect::new(10.0, 20.0, 30.0, 40.0));
        assert_eq!(b.images[0].src, Rect::new(0.0, 0.0, 2.0, 3.0));
    }

    #[test]
    fn image_region_is_collected() {
        let mut list = DisplayList::new();
        let image = Image::new(8, 8, vec![255; 256]);
        list.push(item(DrawCommand::DrawImageRegion(
            image,
            Rect::new(2.0, 3.0, 4.0, 5.0),
            Rect::new(20.0, 30.0, 40.0, 50.0),
        )));

        let b = RenderBatches::lower(&list);
        assert_eq!(b.images.len(), 1);
        assert_eq!(b.images[0].src, Rect::new(2.0, 3.0, 4.0, 5.0));
        assert_eq!(b.images[0].dest, Rect::new(20.0, 30.0, 40.0, 50.0));
    }

    #[test]
    fn clear_after_rects_wipes_them() {
        let mut list = DisplayList::new();
        let paint = Paint::new().color(Color::rgb(0, 255, 0));
        list.push(item(DrawCommand::DrawRect(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            paint,
        )));
        list.push(item(DrawCommand::Clear(Color::rgb(0, 0, 0))));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.clear, Some(Color::rgb(0, 0, 0)));
        assert!(
            b.rects.is_empty(),
            "clear must discard rects drawn before it"
        );
    }

    #[test]
    fn rects_after_clear_survive() {
        let mut list = DisplayList::new();
        let paint = Paint::new().color(Color::rgb(0, 0, 255));
        list.push(item(DrawCommand::Clear(Color::rgb(0, 0, 0))));
        list.push(item(DrawCommand::DrawRect(
            Rect::new(5.0, 5.0, 2.0, 2.0),
            paint,
        )));
        let b = RenderBatches::lower(&list);
        assert_eq!(b.clear, Some(Color::rgb(0, 0, 0)));
        assert_eq!(b.rects.len(), 1);
    }
}
