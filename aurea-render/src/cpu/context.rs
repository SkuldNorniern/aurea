//! CPU drawing context that records commands into a display list.
//!
//! Each draw call is turned into a display item with a node ID, cache key, bounds,
//! opacity, and blend mode so the rasterizer can redraw only what changed.

use super::super::display_list::{CacheKey, DisplayItem, DisplayList, NodeId};
use super::super::renderer::DrawingContext;
use super::super::text::TextRenderer;
use super::super::types::*;
use aurea_foundation::AureaResult;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem::discriminant;
use std::sync::{Arc, LazyLock};

static TEXT_RENDERER: LazyLock<TextRenderer> = LazyLock::new(TextRenderer::new);
const DEFAULT_FONT_FAMILY: &str = "Sans";
const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Snapshot of transform, opacity, clip, and blend mode for save/restore.
struct DrawingState {
    transform: Transform,
    opacity: f32,
    clip: Option<Path>,
    clip_rect: Option<Rect>,
    blend_mode: BlendMode,
}

/// Context that records drawing commands into a display list for the CPU rasterizer.
pub struct CpuDrawingContext {
    display_list: *mut DisplayList,
    /// Sequence counter for this frame's node IDs, reset per frame (a fresh
    /// `CpuDrawingContext` is created in `begin_frame`). Items at the same
    /// position in consecutive frames get the same ID, so display-list
    /// diffing can use index-based identity.
    next_node_id: u64,
    state_stack: Vec<DrawingState>,
    current_transform: Transform,
    current_opacity: f32,
    current_clip: Option<Path>,
    /// The active clip reduced to a physical-pixel rectangle, which is what the
    /// rasterizer can actually enforce. `None` means unclipped; a non-rectangular
    /// clip path leaves it at the enclosing rectangle it was already narrowed to.
    current_clip_rect: Option<Rect>,
    current_blend_mode: BlendMode,
    scale_factor: f32,
    current_interactive_id: Option<super::super::types::InteractiveId>,
    width: u32,
    height: u32,
}

/// Intersection of two rectangles; zero-sized when they do not overlap.
fn intersect_rects(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// Hashes a path's geometry into `hasher`.
///
/// Used for both `DrawPath` commands and the active clip path so that a clip
/// change participates in visual identity.
fn hash_path(path: &Path, hasher: &mut DefaultHasher) {
    for cmd in &path.commands {
        match cmd {
            PathCommand::MoveTo(p) => {
                0u8.hash(hasher);
                p.x.to_bits().hash(hasher);
                p.y.to_bits().hash(hasher);
            }
            PathCommand::LineTo(p) => {
                1u8.hash(hasher);
                p.x.to_bits().hash(hasher);
                p.y.to_bits().hash(hasher);
            }
            PathCommand::QuadTo(c, p) => {
                2u8.hash(hasher);
                c.x.to_bits().hash(hasher);
                c.y.to_bits().hash(hasher);
                p.x.to_bits().hash(hasher);
                p.y.to_bits().hash(hasher);
            }
            PathCommand::CubicTo(c1, c2, p) => {
                3u8.hash(hasher);
                c1.x.to_bits().hash(hasher);
                c1.y.to_bits().hash(hasher);
                c2.x.to_bits().hash(hasher);
                c2.y.to_bits().hash(hasher);
                p.x.to_bits().hash(hasher);
                p.y.to_bits().hash(hasher);
            }
            PathCommand::Close => {
                4u8.hash(hasher);
            }
        }
    }
}

impl CpuDrawingContext {
    /// Creates a context that appends display items to the given display list.
    pub fn new(display_list: *mut DisplayList, width: u32, height: u32) -> Self {
        Self {
            display_list,
            next_node_id: 0,
            state_stack: Vec::new(),
            current_transform: Transform::identity(),
            current_opacity: 1.0,
            current_clip: None,
            current_clip_rect: None,
            current_blend_mode: BlendMode::Normal,
            scale_factor: 1.0,
            current_interactive_id: None,
            width,
            height,
        }
    }

    /// Sets the scale factor used when computing cache keys (e.g. for HiDPI).
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale;
    }

    /// Whether the current transform maps axis-aligned rectangles to
    /// axis-aligned rectangles — i.e. it scales and translates but does not
    /// rotate or skew.
    fn transform_is_axis_aligned(&self) -> bool {
        let t = self.current_transform;
        t.m12.abs() < 1e-6 && t.m21.abs() < 1e-6
    }

    /// Uniform scale implied by the current transform, used where a shape can
    /// only carry a single radius or width.
    fn transform_scale(&self) -> f32 {
        let t = self.current_transform;
        let det = t.m11 * t.m22 - t.m12 * t.m21;
        det.abs().sqrt()
    }

    /// Maps a logical rect through the current transform and into physical
    /// pixels.
    ///
    /// Only correct for an axis-aligned transform; callers that can encounter
    /// rotation check [`Self::transform_is_axis_aligned`] first and record a
    /// path instead.
    fn s_rect(&self, r: Rect) -> Rect {
        let a = self.s_pt(Point::new(r.x, r.y));
        let b = self.s_pt(Point::new(r.x + r.width, r.y + r.height));
        Rect::new(
            a.x.min(b.x),
            a.y.min(b.y),
            (b.x - a.x).abs(),
            (b.y - a.y).abs(),
        )
    }

    /// Maps a logical point through the current transform and into physical
    /// pixels.
    fn s_pt(&self, p: Point) -> Point {
        let t = self.current_transform.map_point(p);
        Point::new(t.x * self.scale_factor, t.y * self.scale_factor)
    }

    /// Maps a logical point through the current transform, staying in logical
    /// coordinates — for commands the rasterizer scales itself.
    fn t_pt(&self, p: Point) -> Point {
        self.current_transform.map_point(p)
    }

    /// Scales a logical length through the current transform into physical
    /// pixels.
    fn s(&self, v: f32) -> f32 {
        v * self.transform_scale() * self.scale_factor
    }

    /// Scale paint properties (stroke width) to physical pixels.
    fn s_paint(&self, paint: &Paint) -> Paint {
        let mut p = paint.clone();
        p.stroke_width *= self.transform_scale() * self.scale_factor;
        p
    }

    /// A copy of `path` with every point mapped through the current transform,
    /// still in logical coordinates.
    fn transformed_path(&self, path: &Path) -> Path {
        if self.current_transform == Transform::identity() {
            return path.clone();
        }
        let commands = path
            .commands
            .iter()
            .map(|cmd| match cmd {
                PathCommand::MoveTo(p) => PathCommand::MoveTo(self.t_pt(*p)),
                PathCommand::LineTo(p) => PathCommand::LineTo(self.t_pt(*p)),
                PathCommand::QuadTo(c, p) => PathCommand::QuadTo(self.t_pt(*c), self.t_pt(*p)),
                PathCommand::CubicTo(c1, c2, p) => {
                    PathCommand::CubicTo(self.t_pt(*c1), self.t_pt(*c2), self.t_pt(*p))
                }
                PathCommand::Close => PathCommand::Close,
            })
            .collect();
        Path { commands }
    }

    /// Builds the transformed outline of a rect, in logical coordinates, for
    /// the rotated/skewed case that a `Rect` command cannot represent.
    fn transformed_rect_path(&self, r: Rect) -> Path {
        let corners = [
            self.t_pt(Point::new(r.x, r.y)),
            self.t_pt(Point::new(r.x + r.width, r.y)),
            self.t_pt(Point::new(r.x + r.width, r.y + r.height)),
            self.t_pt(Point::new(r.x, r.y + r.height)),
        ];
        let mut path = Path::new();
        path.commands.push(PathCommand::MoveTo(corners[0]));
        for corner in &corners[1..] {
            path.commands.push(PathCommand::LineTo(*corner));
        }
        path.commands.push(PathCommand::Close);
        path
    }

    /// Sets the interactive ID for the next drawn shapes (used for hit testing).
    pub fn set_interactive_id(&mut self, id: Option<super::super::types::InteractiveId>) {
        self.current_interactive_id = id;
    }

    /// Draws a rectangle and marks it as interactive with the given ID.
    pub fn draw_interactive_rect(
        &mut self,
        id: super::super::types::InteractiveId,
        rect: Rect,
        paint: &Paint,
    ) -> AureaResult<()> {
        let old_id = self.current_interactive_id;
        self.current_interactive_id = Some(id);
        let result = self.draw_rect(rect, paint);
        self.current_interactive_id = old_id;
        result
    }

    /// Draws a circle and marks it as interactive with the given ID.
    pub fn draw_interactive_circle(
        &mut self,
        id: super::super::types::InteractiveId,
        center: Point,
        radius: f32,
        paint: &Paint,
    ) -> AureaResult<()> {
        let old_id = self.current_interactive_id;
        self.current_interactive_id = Some(id);
        let result = self.draw_circle(center, radius, paint);
        self.current_interactive_id = old_id;
        result
    }

    /// Draws a path and marks it as interactive with the given ID.
    pub fn draw_interactive_path(
        &mut self,
        id: super::super::types::InteractiveId,
        path: &Path,
        paint: &Paint,
    ) -> AureaResult<()> {
        let old_id = self.current_interactive_id;
        self.current_interactive_id = Some(id);
        let result = self.draw_path(path, paint);
        self.current_interactive_id = old_id;
        result
    }

    unsafe fn display_list_mut(&mut self) -> &mut DisplayList {
        unsafe { &mut *self.display_list }
    }

    fn compute_cache_key(&self, command: &super::super::command::DrawCommand) -> CacheKey {
        let mut hasher = DefaultHasher::new();
        match command {
            super::super::command::DrawCommand::Clear(color) => {
                "Clear".hash(&mut hasher);
                color.r.hash(&mut hasher);
                color.g.hash(&mut hasher);
                color.b.hash(&mut hasher);
                color.a.hash(&mut hasher);
            }
            super::super::command::DrawCommand::DrawRect(rect, paint) => {
                "DrawRect".hash(&mut hasher);
                rect.x.to_bits().hash(&mut hasher);
                rect.y.to_bits().hash(&mut hasher);
                rect.width.to_bits().hash(&mut hasher);
                rect.height.to_bits().hash(&mut hasher);
                paint.color.r.hash(&mut hasher);
                paint.color.g.hash(&mut hasher);
                paint.color.b.hash(&mut hasher);
                paint.color.a.hash(&mut hasher);
                paint.style.hash(&mut hasher);
                paint.stroke_width.to_bits().hash(&mut hasher);
            }
            super::super::command::DrawCommand::DrawCircle(center, radius, paint) => {
                "DrawCircle".hash(&mut hasher);
                center.x.to_bits().hash(&mut hasher);
                center.y.to_bits().hash(&mut hasher);
                radius.to_bits().hash(&mut hasher);
                paint.color.r.hash(&mut hasher);
                paint.color.g.hash(&mut hasher);
                paint.color.b.hash(&mut hasher);
                paint.color.a.hash(&mut hasher);
                paint.style.hash(&mut hasher);
                paint.stroke_width.to_bits().hash(&mut hasher);
            }
            super::super::command::DrawCommand::DrawImageRect(image, dest) => {
                "DrawImageRect".hash(&mut hasher);
                image.width.hash(&mut hasher);
                image.height.hash(&mut hasher);
                dest.x.to_bits().hash(&mut hasher);
                dest.y.to_bits().hash(&mut hasher);
                dest.width.to_bits().hash(&mut hasher);
                dest.height.to_bits().hash(&mut hasher);
                // Pixel data is reference-counted; same Arc => same contents.
                (Arc::as_ptr(&image.data) as *const u8 as usize).hash(&mut hasher);
            }
            super::super::command::DrawCommand::DrawImageRegion(image, src, dest) => {
                "DrawImageRegion".hash(&mut hasher);
                image.width.hash(&mut hasher);
                image.height.hash(&mut hasher);
                src.x.to_bits().hash(&mut hasher);
                src.y.to_bits().hash(&mut hasher);
                src.width.to_bits().hash(&mut hasher);
                src.height.to_bits().hash(&mut hasher);
                dest.x.to_bits().hash(&mut hasher);
                dest.y.to_bits().hash(&mut hasher);
                dest.width.to_bits().hash(&mut hasher);
                dest.height.to_bits().hash(&mut hasher);
                (Arc::as_ptr(&image.data) as *const u8 as usize).hash(&mut hasher);
            }
            super::super::command::DrawCommand::FillLinearGradient(grad, rect) => {
                "FillLinearGradient".hash(&mut hasher);
                grad.start.x.to_bits().hash(&mut hasher);
                grad.start.y.to_bits().hash(&mut hasher);
                grad.end.x.to_bits().hash(&mut hasher);
                grad.end.y.to_bits().hash(&mut hasher);
                rect.x.to_bits().hash(&mut hasher);
                rect.y.to_bits().hash(&mut hasher);
                rect.width.to_bits().hash(&mut hasher);
                rect.height.to_bits().hash(&mut hasher);
                for stop in &grad.stops {
                    stop.offset.to_bits().hash(&mut hasher);
                    stop.color.r.hash(&mut hasher);
                    stop.color.g.hash(&mut hasher);
                    stop.color.b.hash(&mut hasher);
                    stop.color.a.hash(&mut hasher);
                }
            }
            super::super::command::DrawCommand::FillRadialGradient(grad, rect) => {
                "FillRadialGradient".hash(&mut hasher);
                grad.center.x.to_bits().hash(&mut hasher);
                grad.center.y.to_bits().hash(&mut hasher);
                grad.radius.to_bits().hash(&mut hasher);
                rect.x.to_bits().hash(&mut hasher);
                rect.y.to_bits().hash(&mut hasher);
                rect.width.to_bits().hash(&mut hasher);
                rect.height.to_bits().hash(&mut hasher);
                for stop in &grad.stops {
                    stop.offset.to_bits().hash(&mut hasher);
                    stop.color.r.hash(&mut hasher);
                    stop.color.g.hash(&mut hasher);
                    stop.color.b.hash(&mut hasher);
                    stop.color.a.hash(&mut hasher);
                }
            }
            super::super::command::DrawCommand::DrawPath(path, paint) => {
                "DrawPath".hash(&mut hasher);
                hash_path(path, &mut hasher);
                paint.color.r.hash(&mut hasher);
                paint.color.g.hash(&mut hasher);
                paint.color.b.hash(&mut hasher);
                paint.color.a.hash(&mut hasher);
                paint.style.hash(&mut hasher);
                paint.stroke_width.to_bits().hash(&mut hasher);
            }
            super::super::command::DrawCommand::DrawGlyphMask(mask, origin, color) => {
                "DrawGlyphMask".hash(&mut hasher);
                // The mask carries a process-unique id that clones share, so
                // unchanged text keeps its key without hashing the coverage
                // bytes — and an evicted mask can never alias a new one.
                mask.id().hash(&mut hasher);
                mask.width.hash(&mut hasher);
                mask.height.hash(&mut hasher);
                origin.x.to_bits().hash(&mut hasher);
                origin.y.to_bits().hash(&mut hasher);
                color.r.hash(&mut hasher);
                color.g.hash(&mut hasher);
                color.b.hash(&mut hasher);
                color.a.hash(&mut hasher);
            }
            _ => {
                discriminant(command).hash(&mut hasher);
            }
        }
        self.current_transform.m11.to_bits().hash(&mut hasher);
        self.current_transform.m12.to_bits().hash(&mut hasher);
        self.current_transform.m13.to_bits().hash(&mut hasher);
        self.current_transform.m21.to_bits().hash(&mut hasher);
        self.current_transform.m22.to_bits().hash(&mut hasher);
        self.current_transform.m23.to_bits().hash(&mut hasher);
        self.current_transform.m31.to_bits().hash(&mut hasher);
        self.current_transform.m32.to_bits().hash(&mut hasher);
        self.current_transform.m33.to_bits().hash(&mut hasher);
        self.current_opacity.to_bits().hash(&mut hasher);
        self.scale_factor.to_bits().hash(&mut hasher);
        // Blend mode and clip affect the pixels an item produces, so they must
        // take part in its visual identity or the damage diff will treat a
        // state-only change as "unchanged".
        self.current_blend_mode.hash(&mut hasher);
        match self.current_clip {
            Some(ref clip) => {
                1u8.hash(&mut hasher);
                hash_path(clip, &mut hasher);
            }
            None => 0u8.hash(&mut hasher),
        }

        CacheKey::from_hash(hasher.finish())
    }

    fn compute_bounds(&self, command: &super::super::command::DrawCommand) -> Rect {
        match command {
            super::super::command::DrawCommand::Clear(_) => Rect::new(0.0, 0.0, f32::MAX, f32::MAX),
            super::super::command::DrawCommand::DrawRect(rect, paint) => {
                let mut bounds = *rect;
                if paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0 {
                    let half_stroke = paint.stroke_width / 2.0;
                    bounds.x -= half_stroke;
                    bounds.y -= half_stroke;
                    bounds.width += paint.stroke_width;
                    bounds.height += paint.stroke_width;
                }
                bounds
            }
            super::super::command::DrawCommand::DrawCircle(center, radius, paint) => {
                let mut bounds = Rect::new(
                    center.x - radius,
                    center.y - radius,
                    radius * 2.0,
                    radius * 2.0,
                );
                if paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0 {
                    let half_stroke = paint.stroke_width / 2.0;
                    bounds.x -= half_stroke;
                    bounds.y -= half_stroke;
                    bounds.width += paint.stroke_width;
                    bounds.height += paint.stroke_width;
                }
                bounds
            }
            super::super::command::DrawCommand::DrawImageRect(_, dest) => *dest,
            super::super::command::DrawCommand::DrawImageRegion(_, _, dest) => *dest,
            super::super::command::DrawCommand::DrawGlyphMask(mask, origin, _) => {
                Rect::new(origin.x, origin.y, mask.width as f32, mask.height as f32)
            }
            super::super::command::DrawCommand::FillLinearGradient(_, rect) => *rect,
            super::super::command::DrawCommand::FillRadialGradient(_, rect) => *rect,
            super::super::command::DrawCommand::DrawPath(path, paint) => {
                // `path` is stored in logical coordinates (P7-F); scale to
                // physical pixels like the other arms before transforming.
                let mut bounds = self.s_rect(super::hit_test::path_bounds(path));
                if paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0 {
                    let half_stroke = paint.stroke_width / 2.0;
                    bounds.x -= half_stroke;
                    bounds.y -= half_stroke;
                    bounds.width += paint.stroke_width;
                    bounds.height += paint.stroke_width;
                }
                bounds
            }
            _ => Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    fn is_opaque(&self, command: &super::super::command::DrawCommand) -> bool {
        match command {
            super::super::command::DrawCommand::Clear(color) => color.a == 255,
            super::super::command::DrawCommand::DrawRect(_, paint) => {
                paint.color.a == 255 && paint.style == PaintStyle::Fill
            }
            super::super::command::DrawCommand::DrawCircle(_, _, paint) => {
                paint.color.a == 255 && paint.style == PaintStyle::Fill
            }
            super::super::command::DrawCommand::DrawImageRect(..)
            | super::super::command::DrawCommand::DrawImageRegion(..) => false,
            super::super::command::DrawCommand::FillLinearGradient(..)
            | super::super::command::DrawCommand::FillRadialGradient(..) => false,
            _ => false,
        }
    }

    fn add_command(&mut self, command: super::super::command::DrawCommand) {
        let cache_key = self.compute_cache_key(&command);
        let bounds = self.compute_bounds(&command);
        let opaque = self.is_opaque(&command) && self.current_opacity >= 1.0;

        let node_id = NodeId(self.next_node_id);
        self.next_node_id += 1;

        let blend = self.current_blend_mode;
        let clip = self.current_clip_rect;
        let item = if let Some(interactive_id) = self.current_interactive_id {
            DisplayItem::new_interactive(
                node_id,
                cache_key,
                bounds,
                opaque,
                interactive_id,
                blend,
                command,
            )
            .with_clip(clip)
            .with_opacity(self.current_opacity)
        } else {
            DisplayItem::new(node_id, cache_key, bounds, opaque, blend, command)
                .with_clip(clip)
                .with_opacity(self.current_opacity)
        };

        unsafe {
            self.display_list_mut().push(item);
        }
    }
}

impl DrawingContext for CpuDrawingContext {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn clear(&mut self, color: Color) -> AureaResult<()> {
        self.add_command(super::super::command::DrawCommand::Clear(color));
        Ok(())
    }

    fn draw_rect(&mut self, rect: Rect, paint: &Paint) -> AureaResult<()> {
        if !self.transform_is_axis_aligned() {
            // A rotated or skewed rectangle is no longer a `Rect`, so record
            // its outline as a path instead of silently drawing it upright.
            let path = self.transformed_rect_path(rect);
            self.add_command(super::super::command::DrawCommand::DrawPath(
                path,
                self.s_paint(paint),
            ));
            return Ok(());
        }
        self.add_command(super::super::command::DrawCommand::DrawRect(
            self.s_rect(rect),
            self.s_paint(paint),
        ));
        Ok(())
    }

    fn draw_circle(&mut self, center: Point, radius: f32, paint: &Paint) -> AureaResult<()> {
        self.add_command(super::super::command::DrawCommand::DrawCircle(
            self.s_pt(center),
            self.s(radius),
            self.s_paint(paint),
        ));
        Ok(())
    }

    fn draw_path(&mut self, path: &Path, paint: &Paint) -> AureaResult<()> {
        // Stored in logical coordinates; the rasterizer applies scale_factor
        // during tessellation, so only the transform is baked in here.
        self.add_command(super::super::command::DrawCommand::DrawPath(
            self.transformed_path(path),
            self.s_paint(paint),
        ));
        Ok(())
    }

    fn draw_text(&mut self, text: &str, point: Point, paint: &Paint) -> AureaResult<()> {
        let font = Font::new(DEFAULT_FONT_FAMILY, DEFAULT_FONT_SIZE);
        self.draw_text_with_font(text, point, &font, paint)
    }

    fn draw_text_with_font(
        &mut self,
        text: &str,
        point: Point,
        font: &Font,
        paint: &Paint,
    ) -> AureaResult<()> {
        if text.is_empty() {
            return Ok(());
        }

        // Rasterize glyphs at physical resolution for sharp HiDPI output.
        let sf = self.scale_factor;
        let physical_font = super::super::text::FontRef::with_size(font, font.size * sf);
        let (mask, ascent, pad) = TEXT_RENDERER.render_text_subpixel(text, physical_font)?;
        if mask.width == 0 || mask.height == 0 {
            return Ok(());
        }

        // Place origin in physical pixel coordinates.
        let px = point.x * sf;
        let py = point.y * sf;
        let origin = Point::new(px - pad, py - ascent - pad);
        self.add_command(super::super::command::DrawCommand::DrawGlyphMask(
            mask,
            origin,
            paint.color,
        ));
        Ok(())
    }

    fn draw_image(&mut self, image: &Image, position: Point) -> AureaResult<()> {
        let sf = self.scale_factor;
        let dest = Rect::new(
            position.x * sf,
            position.y * sf,
            image.width as f32,
            image.height as f32,
        );
        self.add_command(super::super::command::DrawCommand::DrawImageRect(
            image.clone(),
            dest,
        ));
        Ok(())
    }

    fn draw_image_rect(&mut self, image: &Image, dest: Rect) -> AureaResult<()> {
        self.add_command(super::super::command::DrawCommand::DrawImageRect(
            image.clone(),
            self.s_rect(dest),
        ));
        Ok(())
    }

    fn draw_image_region(&mut self, image: &Image, src: Rect, dest: Rect) -> AureaResult<()> {
        self.add_command(super::super::command::DrawCommand::DrawImageRegion(
            image.clone(),
            src,
            self.s_rect(dest),
        ));
        Ok(())
    }

    fn measure_text(&mut self, text: &str, font: &Font) -> AureaResult<TextMetrics> {
        if text.is_empty() {
            return Ok(TextMetrics {
                width: 0.0,
                height: 0.0,
                ascent: 0.0,
                descent: 0.0,
                advance: 0.0,
            });
        }
        // Measure at physical size, then convert back to logical so callers
        // work in logical coordinates regardless of scale factor.
        let sf = self.scale_factor;
        let physical_font = super::super::text::FontRef::with_size(font, font.size * sf);
        let m = TEXT_RENDERER.measure_text(text, physical_font)?;
        Ok(TextMetrics {
            width: m.width / sf,
            height: m.height / sf,
            ascent: m.ascent / sf,
            descent: m.descent / sf,
            advance: m.advance / sf,
        })
    }

    fn save(&mut self) -> AureaResult<()> {
        let transform = self.current_transform;
        let opacity = self.current_opacity;
        let clip = self.current_clip.clone();

        unsafe {
            self.display_list_mut().push_transform(transform);
            self.display_list_mut().push_opacity(opacity);
            if let Some(ref clip_path) = clip {
                self.display_list_mut().push_clip(clip_path.clone());
            }
        }

        self.state_stack.push(DrawingState {
            transform,
            opacity,
            clip,
            clip_rect: self.current_clip_rect,
            blend_mode: self.current_blend_mode,
        });
        Ok(())
    }

    fn restore(&mut self) -> AureaResult<()> {
        if let Some(state) = self.state_stack.pop() {
            self.current_transform = state.transform;
            self.current_opacity = state.opacity;
            self.current_clip = state.clip;
            self.current_clip_rect = state.clip_rect;
            self.current_blend_mode = state.blend_mode;
        }

        unsafe {
            let _ = self.display_list_mut().pop_transform();
            let _ = self.display_list_mut().pop_opacity();
            let _ = self.display_list_mut().pop_clip();
        }
        Ok(())
    }

    fn transform(&mut self, transform: Transform) -> AureaResult<()> {
        // The new transform applies to geometry *before* the transforms
        // already in effect, so that `translate(a); rotate(b)` rotates about
        // the translated origin rather than rotating the translation itself.
        self.current_transform = transform.multiply(self.current_transform);
        Ok(())
    }

    fn clip_rect(&mut self, rect: Rect) -> AureaResult<()> {
        let r = self.s_rect(rect);
        let mut path = Path::new();
        path.commands
            .push(PathCommand::MoveTo(Point::new(r.x, r.y)));
        path.commands
            .push(PathCommand::LineTo(Point::new(r.x + r.width, r.y)));
        path.commands.push(PathCommand::LineTo(Point::new(
            r.x + r.width,
            r.y + r.height,
        )));
        path.commands
            .push(PathCommand::LineTo(Point::new(r.x, r.y + r.height)));
        path.commands.push(PathCommand::Close);
        self.current_clip = Some(path);
        // Clips nest by intersection: a clip inside a clip can only ever show
        // less, never more.
        self.current_clip_rect = Some(match self.current_clip_rect {
            Some(current) => intersect_rects(current, r),
            None => r,
        });
        Ok(())
    }

    fn clip_path(&mut self, path: &Path) -> AureaResult<()> {
        self.current_clip = Some(path.clone());
        // A general path clip is not something the rasterizer can enforce yet.
        // Narrowing to the path's bounding box would let pixels outside the
        // path through, so the enforced clip is left as it was.
        Ok(())
    }

    fn set_alpha(&mut self, alpha: f32) -> AureaResult<()> {
        self.current_opacity = alpha;
        Ok(())
    }

    fn set_blend_mode(&mut self, mode: BlendMode) -> AureaResult<()> {
        self.current_blend_mode = mode;
        Ok(())
    }

    fn fill_linear_gradient(&mut self, gradient: &LinearGradient, rect: Rect) -> AureaResult<()> {
        let sf = self.scale_factor;
        let mut g = gradient.clone();
        g.start = Point::new(g.start.x * sf, g.start.y * sf);
        g.end = Point::new(g.end.x * sf, g.end.y * sf);
        self.add_command(super::super::command::DrawCommand::FillLinearGradient(
            g,
            self.s_rect(rect),
        ));
        Ok(())
    }

    fn fill_radial_gradient(&mut self, gradient: &RadialGradient, rect: Rect) -> AureaResult<()> {
        let sf = self.scale_factor;
        let mut g = gradient.clone();
        g.center = Point::new(g.center.x * sf, g.center.y * sf);
        g.radius *= sf;
        self.add_command(super::super::command::DrawCommand::FillRadialGradient(
            g,
            self.s_rect(rect),
        ));
        Ok(())
    }

    fn hit_test_path(&mut self, path: &Path, point: Point) -> AureaResult<bool> {
        // Both path and point are in logical coordinates; uniform scaling about
        // the origin doesn't change inside/outside, so no scaling is needed.
        Ok(super::hit_test::hit_test_path(path, point))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(list: &mut DisplayList, scale: f32) -> CpuDrawingContext {
        let mut ctx = CpuDrawingContext::new(list, 100, 100);
        ctx.set_scale_factor(scale);
        ctx
    }

    #[test]
    fn translated_bounds_land_at_physical_offset() {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 2.0);
            ctx.translate(10.0, 5.0).expect("translate");
            ctx.draw_rect(Rect::new(0.0, 0.0, 20.0, 20.0), &Paint::default())
                .expect("draw_rect");
        }
        let bounds = list.items()[0].bounds;
        // Logical (10, 5) translation at scale 2 is (20, 10) physical.
        assert!((bounds.x - 20.0).abs() < 1e-4, "x = {}", bounds.x);
        assert!((bounds.y - 10.0).abs() < 1e-4, "y = {}", bounds.y);
        assert!((bounds.width - 40.0).abs() < 1e-4, "w = {}", bounds.width);
    }

    #[test]
    fn untransformed_bounds_are_plain_scaled_geometry() {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 2.0);
            ctx.draw_rect(Rect::new(3.0, 4.0, 20.0, 20.0), &Paint::default())
                .expect("draw_rect");
        }
        let bounds = list.items()[0].bounds;
        assert!((bounds.x - 6.0).abs() < 1e-4, "x = {}", bounds.x);
        assert!((bounds.y - 8.0).abs() < 1e-4, "y = {}", bounds.y);
    }

    #[test]
    fn blend_mode_change_changes_cache_key() {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
            ctx.draw_rect(rect, &Paint::default()).expect("draw_rect");
            ctx.set_blend_mode(BlendMode::Multiply)
                .expect("set_blend_mode");
            ctx.draw_rect(rect, &Paint::default()).expect("draw_rect");
        }
        let items = list.items();
        assert_ne!(items[0].cache_key, items[1].cache_key);
    }

    #[test]
    fn clip_change_changes_cache_key() {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
            ctx.draw_rect(rect, &Paint::default()).expect("draw_rect");
            ctx.clip_rect(Rect::new(0.0, 0.0, 5.0, 5.0))
                .expect("clip_rect");
            ctx.draw_rect(rect, &Paint::default()).expect("draw_rect");
        }
        let items = list.items();
        assert_ne!(items[0].cache_key, items[1].cache_key);
    }
}
