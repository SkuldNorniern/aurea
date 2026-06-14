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

/// One 2-stop gradient fill over a rect. `a[3]` is the kind flag: `0.0` linear,
/// `1.0` radial. 80-byte `#[repr(C)]` (five `vec4`), matching ZenGPU's layout.
///
/// - **Linear:** `a = [start.x, start.y, _, 0.0]`, `b = [end.x, end.y, _, _]`.
/// - **Radial:** `a = [center.x, center.y, radius, 1.0]`, `b` unused.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GradientInstance {
    /// Fill area `[x, y, w, h]` in physical pixels.
    pub rect: [f32; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub color0: [f32; 4],
    pub color1: [f32; 4],
}

impl GradientInstance {
    fn linear(rect: Rect, grad: &LinearGradient) -> Self {
        let (c0, c1) = stop_endpoints(&grad.stops);
        Self {
            rect: [rect.x, rect.y, rect.width, rect.height],
            a: [grad.start.x, grad.start.y, 0.0, 0.0],
            b: [grad.end.x, grad.end.y, 0.0, 0.0],
            color0: color_f32(c0),
            color1: color_f32(c1),
        }
    }

    fn radial(rect: Rect, grad: &RadialGradient) -> Self {
        let (c0, c1) = stop_endpoints(&grad.stops);
        Self {
            rect: [rect.x, rect.y, rect.width, rect.height],
            a: [grad.center.x, grad.center.y, grad.radius, 1.0],
            b: [0.0, 0.0, 0.0, 0.0],
            color0: color_f32(c0),
            color1: color_f32(c1),
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

/// First and last stop colours (the 2-stop reduction of an N-stop gradient).
/// Empty → transparent black; single stop → that colour twice.
fn stop_endpoints(stops: &[GradientStop]) -> (Color, Color) {
    match (stops.first(), stops.last()) {
        (Some(a), Some(b)) => (a.color, b.color),
        _ => (Color::rgba(0, 0, 0, 0), Color::rgba(0, 0, 0, 0)),
    }
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
    /// 2-stop gradient fills in submission order.
    pub gradients: Vec<GradientInstance>,
    /// Images to blit in submission order.
    pub images: Vec<ImageDraw>,
    /// Solid-colour filled circles in submission order.
    pub circles: Vec<CircleInstance>,
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
                    self.gradients.push(GradientInstance::linear(*rect, grad));
                }
                DrawCommand::FillRadialGradient(grad, rect) => {
                    self.gradients.push(GradientInstance::radial(*rect, grad));
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
        assert_eq!(b.gradients[0].color0, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(b.gradients[0].color1, [0.0, 0.0, 1.0, 1.0]);
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
