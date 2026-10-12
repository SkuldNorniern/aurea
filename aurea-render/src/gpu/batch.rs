//! Backend-agnostic 2D batches lowered from a [`DisplayList`].

use crate::capabilities::GpuFeatures;
use crate::numeric::f32_to_u8_clamped;
use crate::scene::command::DrawCommand;
use crate::scene::display_list::{DisplayItem, DisplayList};
use crate::types::{
    BlendMode, Color, GlyphMask, GradientStop, Image, LinearGradient, Paint, PaintStyle, Point,
    RadialGradient, Rect,
};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Weak};

/// One solid-colour rectangle, ready to upload as a GPU instance.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct RectInstance {
    /// `[x, y, width, height]` in physical pixels.
    pub rect: [f32; 4],
    /// Straight RGBA, each channel in `0.0..=1.0`.
    pub color: [f32; 4],
}

impl RectInstance {
    fn from_rect(rect: Rect, color: Color) -> Self {
        Self {
            rect: [rect.x, rect.y, rect.width, rect.height],
            color: [
                f32::from(color.r) / 255.0,
                f32::from(color.g) / 255.0,
                f32::from(color.b) / 255.0,
                f32::from(color.a) / 255.0,
            ],
        }
    }
}

/// One solid-colour filled circle, ready to upload as a GPU instance.
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

/// One gradient fill over a rect.
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
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
        f32::from(c.a) / 255.0,
    ]
}

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
            let lerp = |a: u8, b: u8| {
                f32_to_u8_clamped((f32::from(a) + (f32::from(b) - f32::from(a)) * s).round())
            };
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

/// An image to blit: `image` is the (Arc-backed) pixel source, `dest` the destination rect in
/// physical pixels, `src` the source sub-rect in image pixels, `tint` a straight-RGBA multiply.
#[derive(Debug, Clone)]
pub struct ImageDraw {
    pub image: Image,
    pub dest: Rect,
    pub src: Rect,
    pub tint: Color,
}

/// A cached text-run coverage mask ready for GPU upload.
#[derive(Debug, Clone)]
pub struct TextDraw {
    pub mask: Arc<[u8]>,
    pub rect: Rect,
    pub color: Color,
}

fn drawable_alpha(item: &DisplayItem) -> Option<f32> {
    let alpha = item.opacity.clamp(0.0, 1.0);
    if alpha <= 0.0 { None } else { Some(alpha) }
}

fn needs_of(item: &DisplayItem) -> GpuFeatures {
    let mut needs = match &item.command {
        DrawCommand::Clear(_) | DrawCommand::DrawRect(..) => GpuFeatures::NONE,
        DrawCommand::DrawRoundRect(..) => GpuFeatures {
            rounded_rects: true,
            ..GpuFeatures::NONE
        },
        DrawCommand::DrawCircle(_, _, paint) => GpuFeatures {
            stroked_circles: paint.style == PaintStyle::Stroke,
            ..GpuFeatures::NONE
        },
        DrawCommand::DrawPath(..)
        | DrawCommand::DrawText(..)
        | DrawCommand::DrawTextWithFont(..) => GpuFeatures {
            paths: true,
            ..GpuFeatures::NONE
        },
        DrawCommand::DrawImageRect(..) | DrawCommand::DrawImageRegion(..) => GpuFeatures {
            images: true,
            ..GpuFeatures::NONE
        },
        DrawCommand::DrawGlyphMask(..) => GpuFeatures {
            text: true,
            ..GpuFeatures::NONE
        },
        DrawCommand::FillLinearGradient(..) | DrawCommand::FillRadialGradient(..) => GpuFeatures {
            gradients: true,
            ..GpuFeatures::NONE
        },
    };
    needs.clips = item.clip.is_some();
    needs.blend_modes = item.blend_mode != BlendMode::Normal;
    needs
}

fn representable(item: &DisplayItem) -> bool {
    item.blend_mode == BlendMode::Normal
}

fn stroke_edges(rect: Rect, width: f32) -> [Rect; 4] {
    let half = width / 2.0;
    let (l, t) = (rect.x - half, rect.y - half);
    let (r, b) = (rect.x + rect.width - half, rect.y + rect.height - half);
    let outer_w = rect.width + width;
    let inner_h = (rect.height - width).max(0.0);
    [
        Rect::new(l, t, outer_w, width),
        Rect::new(l, b, outer_w, width),
        Rect::new(l, t + width, width, inner_h),
        Rect::new(r, t + width, width, inner_h),
    ]
}

fn fade(color: Color, factor: f32) -> Color {
    if factor >= 1.0 {
        return color;
    }
    Color::rgba(color.r, color.g, color.b, scale_u8(color.a, factor))
}

fn faded_stops(stops: &[GradientStop], factor: f32) -> Vec<GradientStop> {
    if factor >= 1.0 {
        return stops.to_vec();
    }
    stops
        .iter()
        .map(|stop| GradientStop {
            color: fade(stop.color, factor),
            ..*stop
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn scale_u8(value: u8, factor: f32) -> u8 {
    (f32::from(value) * factor).clamp(0.0, 255.0).round() as u8
}

/// One primitive reference in original display-list submission order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawRef {
    Rect(u32),
    Gradient(u32),
    Image(u32),
    Text(u32),
    Circle(u32),
}

/// A single frame's 2D draw work, lowered from a display list and independent of any GPU backend.
#[derive(Debug, Clone, Default)]
pub struct RenderBatches {
    /// Colour the frame opened with, if it began (or was reset) by a `Clear`.
    pub clear: Option<Color>,
    /// Solid-colour rectangles in submission (painter's-algorithm) order.
    pub rects: Vec<RectInstance>,
    /// LUT-sampled gradient fills in submission order.
    pub gradients: Vec<GradientInstance>,
    /// Images to blit in submission order.
    pub images: Vec<ImageDraw>,
    /// HiDPI text-run masks in submission order.
    pub texts: Vec<TextDraw>,
    /// Solid-colour filled circles in submission order.
    pub circles: Vec<CircleInstance>,
    /// Cross-kind painter order, indexing the per-kind instance arrays above.
    pub order: Vec<DrawRef>,
    /// The clip for each entry in [`Self::order`], in physical pixels.
    pub clips: Vec<Option<Rect>>,
    /// Draws this layer had no form for, and therefore left out of the frame.
    pub dropped: usize,
    /// The kinds of drawing this frame uses, for checking against what a backend supports before it
    /// is asked to draw the frame.
    pub needs: GpuFeatures,
    gradient_lut_cache: HashMap<u64, Weak<[u8]>>,
    text_mask_cache: HashMap<u64, Weak<[u8]>>,
}

impl RenderBatches {
    /// Lower a display list into freshly-allocated batches.
    pub fn lower(list: &DisplayList) -> Self {
        let mut batches = RenderBatches::default();
        batches.lower_into(list);
        batches
    }

    fn push_rect(&mut self, rect: Rect, color: Color) {
        self.order.push(DrawRef::Rect(
            u32::try_from(self.rects.len()).expect("batch count fits in u32"),
        ));
        self.rects.push(RectInstance::from_rect(rect, color));
    }

    pub fn lower_into(&mut self, list: &DisplayList) {
        self.clear = None;
        self.rects.clear();
        self.gradients.clear();
        self.images.clear();
        self.texts.clear();
        self.circles.clear();
        self.order.clear();
        self.clips.clear();
        self.dropped = 0;
        self.needs = GpuFeatures::NONE;
        for item in list.items() {
            self.needs = self.needs.union(needs_of(item));
            self.lower_item(item);
            self.clips.resize(self.order.len(), item.clip);
        }
    }

    fn push_square_round_rect(&mut self, rect: Rect, paint: &Paint, alpha: f32) {
        let color = fade(paint.color, alpha);
        match paint.style {
            PaintStyle::Fill => self.push_rect(rect, color),
            PaintStyle::Stroke if paint.stroke_width > 0.0 => {
                for edge in stroke_edges(rect, paint.stroke_width) {
                    self.push_rect(edge, color);
                }
            }
            PaintStyle::Stroke => {}
        }
    }

    fn lower_item(&mut self, item: &DisplayItem) {
        {
            let Some(alpha) = drawable_alpha(item) else {
                return;
            };
            if !representable(item) {
                self.dropped += 1;
                return;
            }

            match &item.command {
                DrawCommand::Clear(color) => {
                    self.clear = Some(*color);
                    self.rects.clear();
                    self.gradients.clear();
                    self.images.clear();
                    self.texts.clear();
                    self.circles.clear();
                    self.order.clear();
                    self.clips.clear();
                }
                DrawCommand::DrawRect(rect, paint) if paint.style == PaintStyle::Fill => {
                    self.push_rect(*rect, fade(paint.color, alpha));
                }
                DrawCommand::DrawRect(rect, paint)
                    if paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0 =>
                {
                    let color = fade(paint.color, alpha);
                    for edge in stroke_edges(*rect, paint.stroke_width) {
                        self.push_rect(edge, color);
                    }
                }
                DrawCommand::DrawCircle(center, radius, paint)
                    if paint.style == PaintStyle::Fill =>
                {
                    self.order.push(DrawRef::Circle(
                        u32::try_from(self.circles.len()).expect("batch count fits in u32"),
                    ));
                    self.circles.push(CircleInstance::new(
                        *center,
                        *radius,
                        fade(paint.color, alpha),
                    ));
                }
                DrawCommand::FillLinearGradient(grad, rect) => {
                    let lut = self.gradient_lut(&faded_stops(&grad.stops, alpha));
                    self.order.push(DrawRef::Gradient(
                        u32::try_from(self.gradients.len()).expect("batch count fits in u32"),
                    ));
                    self.gradients
                        .push(GradientInstance::linear(*rect, grad, lut));
                }
                DrawCommand::FillRadialGradient(grad, rect) => {
                    let lut = self.gradient_lut(&faded_stops(&grad.stops, alpha));
                    self.order.push(DrawRef::Gradient(
                        u32::try_from(self.gradients.len()).expect("batch count fits in u32"),
                    ));
                    self.gradients
                        .push(GradientInstance::radial(*rect, grad, lut));
                }
                DrawCommand::DrawImageRect(image, dest)
                    if valid_rgba_image(image.width, image.height, &image.data) =>
                {
                    self.order.push(DrawRef::Image(
                        u32::try_from(self.images.len()).expect("batch count fits in u32"),
                    ));
                    self.images.push(ImageDraw {
                        image: image.clone(),
                        dest: *dest,
                        src: Rect::new(0.0, 0.0, image.width as f32, image.height as f32),
                        tint: fade(Color::rgb(255, 255, 255), alpha),
                    });
                }
                DrawCommand::DrawImageRegion(image, src, dest)
                    if valid_rgba_image(image.width, image.height, &image.data) =>
                {
                    self.order.push(DrawRef::Image(
                        u32::try_from(self.images.len()).expect("batch count fits in u32"),
                    ));
                    self.images.push(ImageDraw {
                        image: image.clone(),
                        dest: *dest,
                        src: *src,
                        tint: fade(Color::rgb(255, 255, 255), alpha),
                    });
                }
                DrawCommand::DrawGlyphMask(mask, origin, color) => {
                    if let Some(rgba) = self.text_mask(mask) {
                        self.order.push(DrawRef::Text(
                            u32::try_from(self.texts.len()).expect("batch count fits in u32"),
                        ));
                        self.texts.push(TextDraw {
                            mask: rgba,
                            rect: Rect::new(
                                origin.x,
                                origin.y,
                                mask.width as f32,
                                mask.height as f32,
                            ),
                            color: fade(*color, alpha),
                        });
                    }
                }
                DrawCommand::DrawRoundRect(rect, _, paint) => {
                    self.push_square_round_rect(*rect, paint, alpha);
                }
                _ => self.dropped += 1,
            }
        }
    }

    /// True when there's nothing to clear and nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.clear.is_none()
            && self.rects.is_empty()
            && self.gradients.is_empty()
            && self.images.is_empty()
            && self.texts.is_empty()
            && self.circles.is_empty()
            && self.order.is_empty()
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

    fn text_mask(&mut self, mask: &GlyphMask) -> Option<Arc<[u8]>> {
        let pixel_count = (mask.width as usize).checked_mul(mask.height as usize)?;
        if mask.coverage.len() != pixel_count.checked_mul(3)? {
            return None;
        }
        let key = mask.id();
        if let Some(rgba) = self.text_mask_cache.get(&key).and_then(Weak::upgrade) {
            return Some(rgba);
        }
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        let (pixels, _) = mask.coverage.as_chunks::<3>();
        for &[r, g, b] in pixels {
            rgba.extend_from_slice(&[r, g, b, r.max(g).max(b)]);
        }
        let rgba: Arc<[u8]> = rgba.into();
        self.text_mask_cache.insert(key, Arc::downgrade(&rgba));
        Some(rgba)
    }
}

fn valid_rgba_image(width: u32, height: u32, data: &[u8]) -> bool {
    width > 0
        && height > 0
        && (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            == Some(data.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowering_records_what_the_frame_needs() {
        let mut list = DisplayList::new();
        list.push(item(DrawCommand::DrawRect(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            Paint::new(),
        )));
        assert!(RenderBatches::lower(&list).needs.is_empty());
        list.push(item(DrawCommand::DrawRoundRect(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            2.0,
            Paint::new(),
        )));
        let needs = RenderBatches::lower(&list).needs;
        assert!(needs.rounded_rects && !needs.paths && !needs.images);
    }
    use crate::scene::command::DrawCommand;
    use crate::scene::display_list::{CacheKey, DisplayIndex, DisplayItem};
    use crate::types::Path;
    use crate::types::{BlendMode, Paint, Rect};

    fn item(command: DrawCommand) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
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
    fn a_stroke_with_no_width_draws_nothing() {
        let mut list = DisplayList::new();
        let paint = Paint::new().style(PaintStyle::Stroke).stroke_width(0.0);
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
    fn glyph_mask_is_converted_to_rgba_text_draw() {
        let mut list = DisplayList::new();
        list.push(item(DrawCommand::DrawGlyphMask(
            GlyphMask::new(1, 1, vec![10, 20, 30].into()),
            Point::new(4.0, 5.0),
            Color::rgb(200, 100, 50),
        )));

        let b = RenderBatches::lower(&list);
        assert_eq!(b.texts.len(), 1);
        assert_eq!(b.texts[0].rect, Rect::new(4.0, 5.0, 1.0, 1.0));
        assert_eq!(&*b.texts[0].mask, &[10, 20, 30, 30]);
    }

    #[test]
    fn repeated_lowering_reuses_text_mask_arc() {
        let mut list = DisplayList::new();
        list.push(item(DrawCommand::DrawGlyphMask(
            GlyphMask::new(1, 1, vec![255, 255, 255].into()),
            Point::new(0.0, 0.0),
            Color::rgb(255, 255, 255),
        )));
        let mut batches = RenderBatches::default();
        batches.lower_into(&list);
        let first = Arc::clone(&batches.texts[0].mask);
        batches.lower_into(&list);
        assert!(Arc::ptr_eq(&first, &batches.texts[0].mask));
    }

    #[test]
    fn malformed_glyph_mask_is_skipped() {
        let mut list = DisplayList::new();
        list.push(item(DrawCommand::DrawGlyphMask(
            GlyphMask::new(2, 1, vec![255, 255, 255].into()),
            Point::new(0.0, 0.0),
            Color::rgb(255, 255, 255),
        )));
        assert!(RenderBatches::lower(&list).texts.is_empty());
    }

    #[test]
    fn cross_kind_order_matches_display_list() {
        let mut list = DisplayList::new();
        list.push(item(DrawCommand::DrawCircle(
            Point::new(5.0, 5.0),
            3.0,
            Paint::new(),
        )));
        list.push(item(DrawCommand::DrawRect(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Paint::new(),
        )));
        list.push(item(DrawCommand::DrawImageRect(
            Image::new(1, 1, vec![255; 4]),
            Rect::new(0.0, 0.0, 1.0, 1.0),
        )));

        let b = RenderBatches::lower(&list);
        assert_eq!(
            b.order,
            vec![DrawRef::Circle(0), DrawRef::Rect(0), DrawRef::Image(0)]
        );
    }

    #[test]
    fn clear_resets_cross_kind_order() {
        let mut list = DisplayList::new();
        list.push(item(DrawCommand::DrawCircle(
            Point::new(5.0, 5.0),
            3.0,
            Paint::new(),
        )));
        list.push(item(DrawCommand::Clear(Color::rgb(0, 0, 0))));
        list.push(item(DrawCommand::DrawRect(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Paint::new(),
        )));

        let b = RenderBatches::lower(&list);
        assert_eq!(b.order, vec![DrawRef::Rect(0)]);
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
    fn a_stroked_rect_becomes_four_edges() {
        let mut list = DisplayList::new();
        let paint = Paint::new()
            .color(Color::rgb(255, 0, 0))
            .style(PaintStyle::Stroke)
            .stroke_width(2.0);
        list.push(item(DrawCommand::DrawRect(
            Rect::new(10.0, 10.0, 100.0, 50.0),
            paint,
        )));

        let b = RenderBatches::lower(&list);

        assert_eq!(b.rects.len(), 4, "one rect per edge");
        assert_eq!(b.order.len(), 4);
        assert_eq!(b.clips.len(), b.order.len(), "a clip for every draw");
    }

    #[test]
    fn the_edges_of_a_stroked_rect_do_not_overlap() {
        let edges = stroke_edges(Rect::new(10.0, 10.0, 100.0, 50.0), 2.0);
        for (i, a) in edges.iter().enumerate() {
            for b in edges.iter().skip(i + 1) {
                let w = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
                let h = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
                assert!(w <= 0.0 || h <= 0.0, "{a:?} and {b:?} overlap");
            }
        }
    }

    #[test]
    fn a_stroke_straddles_the_edge_it_outlines() {
        let edges = stroke_edges(Rect::new(10.0, 10.0, 100.0, 50.0), 4.0);
        let top = edges[0];

        assert_eq!(top.y, 8.0, "half the width above the top edge");
        assert_eq!(top.x, 8.0);
        assert_eq!(top.width, 104.0, "wide enough to carry both corners");
    }

    #[test]
    fn a_stroke_thicker_than_the_rect_has_no_sides() {
        let edges = stroke_edges(Rect::new(0.0, 0.0, 20.0, 4.0), 10.0);

        assert_eq!(edges[2].height, 0.0);
        assert_eq!(edges[3].height, 0.0);
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

    fn placed(command: DrawCommand, bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(0),
            bounds,
            false,
            BlendMode::Normal,
            command,
        )
    }

    fn red_rect(bounds: Rect) -> DrawCommand {
        DrawCommand::DrawRect(bounds, Paint::new().color(Color::rgb(255, 0, 0)))
    }

    #[test]
    fn opacity_is_folded_into_the_colour() {
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut list = DisplayList::new();
        list.push(placed(red_rect(bounds), bounds).with_opacity(0.5));

        let b = RenderBatches::lower(&list);
        assert_eq!(b.rects.len(), 1);
        let alpha = b.rects[0].color[3];
        assert!(
            (alpha - 0.5).abs() < 0.01,
            "expected about half alpha, got {alpha}"
        );
    }

    #[test]
    fn a_fully_transparent_item_is_dropped() {
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut list = DisplayList::new();
        list.push(placed(red_rect(bounds), bounds).with_opacity(0.0));

        assert!(RenderBatches::lower(&list).rects.is_empty());
    }

    #[test]
    fn opacity_reaches_gradients_and_images_too() {
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let gradient = LinearGradient {
            start: Point::new(0.0, 0.0),
            end: Point::new(10.0, 0.0),
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

        let mut list = DisplayList::new();
        list.push(
            placed(DrawCommand::FillLinearGradient(gradient, bounds), bounds).with_opacity(0.5),
        );

        let b = RenderBatches::lower(&list);
        assert_eq!(b.gradients.len(), 1, "the gradient should still be drawn");
    }

    #[test]
    fn a_clipped_item_is_drawn_and_carries_its_clip() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 100.0);
        let clip = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut list = DisplayList::new();
        list.push(placed(red_rect(bounds), bounds).with_clip(Some(clip)));

        let b = RenderBatches::lower(&list);
        assert_eq!(b.rects.len(), 1, "the item should still be drawn");
        assert_eq!(b.clips, vec![Some(clip)], "with its clip alongside");
    }

    #[test]
    fn an_unclipped_item_carries_no_clip() {
        let bounds = Rect::new(10.0, 10.0, 10.0, 10.0);
        let mut list = DisplayList::new();
        list.push(placed(red_rect(bounds), bounds));

        assert_eq!(RenderBatches::lower(&list).clips, vec![None]);
    }

    #[test]
    fn every_draw_has_a_clip_entry() {
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let clip = Rect::new(0.0, 0.0, 5.0, 5.0);

        let mut list = DisplayList::new();
        list.push(placed(red_rect(bounds), bounds));
        list.push(placed(red_rect(bounds), bounds).with_clip(Some(clip)));
        list.push(placed(
            DrawCommand::DrawPath(Path::new(), Paint::new()),
            bounds,
        ));
        list.push(placed(red_rect(bounds), bounds));

        let b = RenderBatches::lower(&list);
        assert_eq!(b.order.len(), b.clips.len());
        assert_eq!(b.clips, vec![None, Some(clip), None]);
    }

    #[test]
    fn draws_with_no_gpu_form_are_counted() {
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut list = DisplayList::new();
        list.push(placed(red_rect(bounds), bounds));
        list.push(placed(
            DrawCommand::DrawPath(Path::new(), Paint::new()),
            bounds,
        ));
        let mut blended = placed(red_rect(bounds), bounds);
        blended.blend_mode = BlendMode::Multiply;
        list.push(blended);

        let b = RenderBatches::lower(&list);

        assert_eq!(b.dropped, 2, "the path and the blended rect");
        assert_eq!(b.rects.len(), 1);
    }

    #[test]
    fn the_dropped_count_is_per_frame() {
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut with_path = DisplayList::new();
        with_path.push(placed(
            DrawCommand::DrawPath(Path::new(), Paint::new()),
            bounds,
        ));

        let mut batches = RenderBatches::default();
        batches.lower_into(&with_path);
        batches.lower_into(&with_path);

        assert_eq!(batches.dropped, 1, "one per frame, not two");

        let clean = DisplayList::new();
        batches.lower_into(&clean);
        assert_eq!(
            batches.dropped, 0,
            "a frame that drops nothing reports none"
        );
    }

    #[test]
    fn a_transparent_draw_is_not_counted_as_dropped() {
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut item = placed(red_rect(bounds), bounds);
        item.opacity = 0.0;

        let mut list = DisplayList::new();
        list.push(item);

        assert_eq!(RenderBatches::lower(&list).dropped, 0);
    }

    #[test]
    fn a_non_normal_blend_mode_is_skipped() {
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut item = placed(red_rect(bounds), bounds);
        item.blend_mode = BlendMode::Multiply;

        let mut list = DisplayList::new();
        list.push(item);

        assert!(
            RenderBatches::lower(&list).rects.is_empty(),
            "drawing it as Normal would be the wrong picture"
        );
    }
}
