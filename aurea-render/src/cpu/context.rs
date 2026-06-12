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
use std::sync::LazyLock;

static TEXT_RENDERER: LazyLock<TextRenderer> = LazyLock::new(TextRenderer::new);
const DEFAULT_FONT_FAMILY: &str = "Sans";
const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Snapshot of transform, opacity, clip, and blend mode for save/restore.
struct DrawingState {
    transform: Transform,
    opacity: f32,
    clip: Option<Path>,
    blend_mode: BlendMode,
}

/// Context that records drawing commands into a display list for the CPU rasterizer.
pub struct CpuDrawingContext {
    display_list: *mut DisplayList,
    current_node_id: NodeId,
    state_stack: Vec<DrawingState>,
    current_transform: Transform,
    current_opacity: f32,
    current_clip: Option<Path>,
    current_blend_mode: BlendMode,
    scale_factor: f32,
    current_interactive_id: Option<super::super::types::InteractiveId>,
    width: u32,
    height: u32,
}

impl CpuDrawingContext {
    /// Creates a context that appends display items to the given display list.
    pub fn new(display_list: *mut DisplayList, width: u32, height: u32) -> Self {
        Self {
            display_list,
            current_node_id: NodeId::new(),
            state_stack: Vec::new(),
            current_transform: Transform::identity(),
            current_opacity: 1.0,
            current_clip: None,
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

    /// Scale a logical rect to physical pixels.
    fn s_rect(&self, r: Rect) -> Rect {
        let s = self.scale_factor;
        Rect::new(r.x * s, r.y * s, r.width * s, r.height * s)
    }

    /// Scale a logical point to physical pixels.
    fn s_pt(&self, p: Point) -> Point {
        Point::new(p.x * self.scale_factor, p.y * self.scale_factor)
    }

    /// Scale a logical scalar to physical pixels.
    fn s(&self, v: f32) -> f32 {
        v * self.scale_factor
    }

    /// Scale a path from logical to physical pixel coordinates.
    fn s_path(&self, path: &Path) -> Path {
        let sf = self.scale_factor;
        let mut out = Path::new();
        for cmd in &path.commands {
            out.commands.push(match cmd {
                super::super::types::PathCommand::MoveTo(p) => {
                    super::super::types::PathCommand::MoveTo(Point::new(p.x * sf, p.y * sf))
                }
                super::super::types::PathCommand::LineTo(p) => {
                    super::super::types::PathCommand::LineTo(Point::new(p.x * sf, p.y * sf))
                }
                super::super::types::PathCommand::QuadTo(c, p) => {
                    super::super::types::PathCommand::QuadTo(
                        Point::new(c.x * sf, c.y * sf),
                        Point::new(p.x * sf, p.y * sf),
                    )
                }
                super::super::types::PathCommand::CubicTo(c1, c2, p) => {
                    super::super::types::PathCommand::CubicTo(
                        Point::new(c1.x * sf, c1.y * sf),
                        Point::new(c2.x * sf, c2.y * sf),
                        Point::new(p.x * sf, p.y * sf),
                    )
                }
                super::super::types::PathCommand::Close => super::super::types::PathCommand::Close,
            });
        }
        out
    }

    /// Scale paint properties (stroke width) to physical pixels.
    fn s_paint(&self, paint: &Paint) -> Paint {
        let mut p = paint.clone();
        p.stroke_width *= self.scale_factor;
        p
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
                let sample_len = (image.data.len()).min(256);
                for i in 0..sample_len {
                    image.data[i].hash(&mut hasher);
                }
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
                let sample_len = (image.data.len()).min(256);
                for i in 0..sample_len {
                    image.data[i].hash(&mut hasher);
                }
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
                for cmd in &path.commands {
                    match cmd {
                        super::super::types::PathCommand::MoveTo(p) => {
                            0u8.hash(&mut hasher);
                            p.x.to_bits().hash(&mut hasher);
                            p.y.to_bits().hash(&mut hasher);
                        }
                        super::super::types::PathCommand::LineTo(p) => {
                            1u8.hash(&mut hasher);
                            p.x.to_bits().hash(&mut hasher);
                            p.y.to_bits().hash(&mut hasher);
                        }
                        super::super::types::PathCommand::QuadTo(c, p) => {
                            2u8.hash(&mut hasher);
                            c.x.to_bits().hash(&mut hasher);
                            c.y.to_bits().hash(&mut hasher);
                            p.x.to_bits().hash(&mut hasher);
                            p.y.to_bits().hash(&mut hasher);
                        }
                        super::super::types::PathCommand::CubicTo(c1, c2, p) => {
                            3u8.hash(&mut hasher);
                            c1.x.to_bits().hash(&mut hasher);
                            c1.y.to_bits().hash(&mut hasher);
                            c2.x.to_bits().hash(&mut hasher);
                            c2.y.to_bits().hash(&mut hasher);
                            p.x.to_bits().hash(&mut hasher);
                            p.y.to_bits().hash(&mut hasher);
                        }
                        super::super::types::PathCommand::Close => {
                            4u8.hash(&mut hasher);
                        }
                    }
                }
                paint.color.r.hash(&mut hasher);
                paint.color.g.hash(&mut hasher);
                paint.color.b.hash(&mut hasher);
                paint.color.a.hash(&mut hasher);
                paint.style.hash(&mut hasher);
                paint.stroke_width.to_bits().hash(&mut hasher);
            }
            super::super::command::DrawCommand::DrawGlyphMask(mask, origin, color) => {
                "DrawGlyphMask".hash(&mut hasher);
                // The coverage buffer comes from the glyph/run LRU caches, so its
                // Arc pointer is a stable identity for unchanged text — avoids
                // hashing (or debug-formatting) the coverage bytes themselves.
                (std::sync::Arc::as_ptr(&mask.coverage) as *const u8 as usize).hash(&mut hasher);
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
                std::mem::discriminant(command).hash(&mut hasher);
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

        CacheKey::from_hash(hasher.finish())
    }

    fn transform_point(&self, point: Point) -> Point {
        let x = self.current_transform.m11 * point.x
            + self.current_transform.m12 * point.y
            + self.current_transform.m13;
        let y = self.current_transform.m21 * point.x
            + self.current_transform.m22 * point.y
            + self.current_transform.m23;
        Point::new(x, y)
    }

    fn transform_rect(&self, rect: Rect) -> Rect {
        let top_left = self.transform_point(Point::new(rect.x, rect.y));
        let top_right = self.transform_point(Point::new(rect.x + rect.width, rect.y));
        let bottom_left = self.transform_point(Point::new(rect.x, rect.y + rect.height));
        let bottom_right =
            self.transform_point(Point::new(rect.x + rect.width, rect.y + rect.height));

        let min_x = top_left
            .x
            .min(top_right.x)
            .min(bottom_left.x)
            .min(bottom_right.x);
        let max_x = top_left
            .x
            .max(top_right.x)
            .max(bottom_left.x)
            .max(bottom_right.x);
        let min_y = top_left
            .y
            .min(top_right.y)
            .min(bottom_left.y)
            .min(bottom_right.y);
        let max_y = top_left
            .y
            .max(top_right.y)
            .max(bottom_left.y)
            .max(bottom_right.y);

        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
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
                self.transform_rect(bounds)
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
                self.transform_rect(bounds)
            }
            super::super::command::DrawCommand::DrawImageRect(_, dest) => {
                self.transform_rect(*dest)
            }
            super::super::command::DrawCommand::DrawImageRegion(_, _, dest) => {
                self.transform_rect(*dest)
            }
            super::super::command::DrawCommand::DrawGlyphMask(mask, origin, _) => self
                .transform_rect(Rect::new(
                    origin.x,
                    origin.y,
                    mask.width as f32,
                    mask.height as f32,
                )),
            super::super::command::DrawCommand::FillLinearGradient(_, rect) => {
                self.transform_rect(*rect)
            }
            super::super::command::DrawCommand::FillRadialGradient(_, rect) => {
                self.transform_rect(*rect)
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

        let blend = self.current_blend_mode;
        let item = if let Some(interactive_id) = self.current_interactive_id {
            DisplayItem::new_interactive(
                self.current_node_id,
                cache_key,
                bounds,
                opaque,
                interactive_id,
                blend,
                command,
            )
        } else {
            DisplayItem::new(
                self.current_node_id,
                cache_key,
                bounds,
                opaque,
                blend,
                command,
            )
        };

        unsafe {
            self.display_list_mut().push(item);
        }
        self.current_node_id = NodeId::new();
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
        self.add_command(super::super::command::DrawCommand::DrawPath(
            self.s_path(path),
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
        let physical_font = Font::new(font.family.trim(), font.size * sf);
        let (mask, ascent, pad) = TEXT_RENDERER.render_text_subpixel(text, &physical_font)?;
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
        let physical_font = Font::new(font.family.trim(), font.size * sf);
        let m = TEXT_RENDERER.measure_text(text, &physical_font)?;
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

        self.state_stack.push(DrawingState {
            transform,
            opacity,
            clip: clip.clone(),
            blend_mode: self.current_blend_mode,
        });

        unsafe {
            self.display_list_mut().push_transform(transform);
            self.display_list_mut().push_opacity(opacity);
            if let Some(ref clip_path) = clip {
                self.display_list_mut().push_clip(clip_path.clone());
            }
        }
        Ok(())
    }

    fn restore(&mut self) -> AureaResult<()> {
        if let Some(state) = self.state_stack.pop() {
            self.current_transform = state.transform;
            self.current_opacity = state.opacity;
            self.current_clip = state.clip;
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
        self.current_transform = self.current_transform.multiply(transform);
        Ok(())
    }

    fn clip_rect(&mut self, rect: Rect) -> AureaResult<()> {
        let r = self.s_rect(rect);
        let mut path = Path::new();
        path.commands
            .push(super::super::types::PathCommand::MoveTo(Point::new(
                r.x, r.y,
            )));
        path.commands
            .push(super::super::types::PathCommand::LineTo(Point::new(
                r.x + r.width,
                r.y,
            )));
        path.commands
            .push(super::super::types::PathCommand::LineTo(Point::new(
                r.x + r.width,
                r.y + r.height,
            )));
        path.commands
            .push(super::super::types::PathCommand::LineTo(Point::new(
                r.x,
                r.y + r.height,
            )));
        path.commands.push(super::super::types::PathCommand::Close);
        self.current_clip = Some(path);
        Ok(())
    }

    fn clip_path(&mut self, path: &Path) -> AureaResult<()> {
        self.current_clip = Some(path.clone());
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
        // Path coords are physical; convert the (logical) test point to physical.
        let physical_point = self.s_pt(point);
        Ok(super::hit_test::hit_test_path(
            &self.s_path(path),
            physical_point,
        ))
    }
}
