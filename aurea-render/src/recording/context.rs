//! Records draw calls into a display list.

use crate::renderer::DrawingContext;
use crate::scene::command::{DrawCommand, quad_bounds};
use crate::scene::display_list::{CacheKey, DisplayIndex, DisplayItem, DisplayList};
use crate::scene::hit_test::{hit_test_path, path_bounds};
use crate::text::{
    FaceData, FaceId, FontRef, GlyphCoverage, LayoutOptions, ShapedText, TextLayout, TextRenderer,
};
use crate::types::*;
use aurea_foundation::{AureaError, AureaResult, Platform};
use std::collections::hash_map::DefaultHasher;
use std::f32::consts::{SQRT_2, TAU};
use std::hash::{Hash, Hasher};
use std::mem::discriminant;
use std::sync::{Arc, LazyLock};

static TEXT_RENDERER: LazyLock<TextRenderer> = LazyLock::new(TextRenderer::new);
const DEFAULT_FONT_FAMILY: &str = "Sans";

/// Measures `text` in `font` at logical size, with the same text engine and caches the CPU renderer
/// draws with, so what is measured is what is drawn.
pub fn measure_text(text: &str, font: &Font) -> AureaResult<TextMetrics> {
    if text.is_empty() {
        return Ok(TextMetrics {
            width: 0.0,
            height: 0.0,
            ascent: 0.0,
            descent: 0.0,
            advance: 0.0,
        });
    }
    TEXT_RENDERER.measure_text(text, FontRef::with_size(font, font.size))
}

/// `text` in `font` shaped as one line at logical size, by the engine that draws it: glyph ids, the
/// faces drawing them (fallbacks included), clusters and positions.
pub fn shape_text(text: &str, font: &Font) -> AureaResult<Arc<ShapedText>> {
    TEXT_RENDERER.shape(text, FontRef::with_size(font, font.size))
}

/// `text` in `font` laid out in lines by `options`, at logical size: for paragraphs, wrapping and
/// editors.
pub fn layout_text(text: &str, font: &Font, options: LayoutOptions) -> AureaResult<TextLayout> {
    TEXT_RENDERER.layout(text, FontRef::with_size(font, font.size), options)
}

/// A shaped glyph's outline at `size` pixels, pen at the origin on the baseline; `None` for one
/// with no outline, like a space.
pub fn glyph_outline(face: FaceId, glyph: u16, size: f32) -> AureaResult<Option<Path>> {
    TEXT_RENDERER.glyph_outline(face, glyph, size)
}

/// A shaped glyph's grayscale coverage at `size` pixels.
pub fn glyph_coverage(face: FaceId, glyph: u16, size: f32) -> AureaResult<GlyphCoverage> {
    TEXT_RENDERER.glyph_coverage(face, glyph, size)
}

/// `font`'s own metrics at its size: line gap, cap height, underline and the rest, no canvas needed.
pub fn font_metrics(font: &Font) -> AureaResult<FontMetrics> {
    TEXT_RENDERER.font_metrics(FontRef::with_size(font, font.size))
}

/// Whether `font`'s own face has a glyph for `ch`, before any fallback face steps in.
pub fn font_covers(font: &Font, ch: char) -> AureaResult<bool> {
    TEXT_RENDERER.font_covers(FontRef::with_size(font, font.size), ch)
}

/// The font file behind a shaped glyph's face.
pub fn face_data(face: FaceId) -> AureaResult<Arc<FaceData>> {
    TEXT_RENDERER.face_data(face)
}
const DEFAULT_FONT_SIZE: f32 = 16.0;

#[derive(Clone, Copy)]
struct DrawingState {
    transform: Transform,
    opacity: f32,
    clip_rect: Option<Rect>,
    clip_paths: usize,
    blend_mode: BlendMode,
    sampling: Sampling,
    pixel_snap: bool,
}

const INLINE_SAVES: usize = 32;

struct SaveStack {
    inline: [Option<DrawingState>; INLINE_SAVES],
    len: usize,
    spill: Vec<DrawingState>,
}

impl SaveStack {
    const fn new() -> Self {
        Self {
            inline: [None; INLINE_SAVES],
            len: 0,
            spill: Vec::new(),
        }
    }

    fn push(&mut self, state: DrawingState) {
        match self.inline.get_mut(self.len) {
            Some(slot) => {
                *slot = Some(state);
                self.len += 1;
            }
            None => self.spill.push(state),
        }
    }

    fn pop(&mut self) -> Option<DrawingState> {
        if let Some(state) = self.spill.pop() {
            return Some(state);
        }
        self.len = self.len.checked_sub(1)?;
        self.inline[self.len].take()
    }
}

/// Records drawing commands into a display list.
///
/// ```compile_fail
/// use aurea_render::{CpuRasterizer, Renderer};
///
/// let mut renderer = CpuRasterizer::new(64, 64);
/// let ctx = renderer.begin_frame().expect("begin_frame");
/// drop(renderer);
/// let _ = ctx;
/// ```
/// ```compile_fail
/// use aurea_render::{CpuRasterizer, Renderer};
///
/// let mut renderer = CpuRasterizer::new(64, 64);
/// let ctx = renderer.begin_frame().expect("begin_frame");
/// renderer.end_frame().expect("end_frame");
/// let _ = ctx;
/// ```
/// ```rust
/// use aurea_render::{CpuRasterizer, Renderer};
///
/// let mut renderer = CpuRasterizer::new(64, 64);
/// {
///     let mut ctx = renderer.begin_frame().expect("begin_frame");
///     let _ = ctx.width();
/// }
/// renderer.end_frame().expect("end_frame");
/// ```
pub struct RecordingContext<'list> {
    display_list: &'list mut DisplayList,
    next_index: u64,
    state_stack: SaveStack,
    current_transform: Transform,
    current_opacity: f32,
    current_clip_rect: Option<Rect>,
    clip_paths: Vec<Path>,
    current_clip_paths: Option<Arc<[Path]>>,
    current_clip_paths_hash: u64,
    path_clips: bool,
    current_blend_mode: BlendMode,
    current_sampling: Sampling,
    current_pixel_snap: bool,
    scale_factor: f32,
    current_interactive_id: Option<InteractiveId>,
    width: u32,
    height: u32,
}

fn axis_aligned_rect(path: &Path) -> Option<Rect> {
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

fn stroke_reach(paint: &Paint) -> f32 {
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

fn rect_path(rect: Rect) -> Path {
    let mut path = Path::new();
    path.commands.extend_from_slice(&[
        PathCommand::MoveTo(Point::new(rect.x, rect.y)),
        PathCommand::LineTo(Point::new(rect.x + rect.width, rect.y)),
        PathCommand::LineTo(Point::new(rect.x + rect.width, rect.y + rect.height)),
        PathCommand::LineTo(Point::new(rect.x, rect.y + rect.height)),
        PathCommand::Close,
    ]);
    path
}

fn snap_rect(rect: Rect, paint: &Paint) -> (Rect, f32) {
    let span = |start: f32, len: f32, inset: f32| {
        if len <= 0.0 {
            return (start.round(), 0.0);
        }
        let outer = (start - inset).round();
        let end = (start + len + inset).round();
        let size = (end - outer).max(1.0 + 2.0 * inset);
        (outer + inset, size - 2.0 * inset)
    };
    let stroke = paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0;
    let width = if stroke {
        paint.stroke_width.round().max(1.0)
    } else {
        paint.stroke_width
    };
    let inset = if stroke { width / 2.0 } else { 0.0 };
    let (x, w) = span(rect.x, rect.width, inset);
    let (y, h) = span(rect.y, rect.height, inset);
    (Rect::new(x, y, w, h), width)
}

/// A straight rule snapped: its ends round on their own like a fill's edges, its thickness rounds
/// as a whole so every rule of one width comes out alike.
fn snap_rule(rect: Rect, horizontal: bool) -> Rect {
    let keep = |start: f32, len: f32| {
        let size = len.round().max(1.0);
        ((start + (len - size) / 2.0).round(), size)
    };
    let edges = |start: f32, len: f32| {
        let outer = start.round();
        (outer, ((start + len).round() - outer).max(1.0))
    };
    let ((x, w), (y, h)) = if horizontal {
        (edges(rect.x, rect.width), keep(rect.y, rect.height))
    } else {
        (keep(rect.x, rect.width), edges(rect.y, rect.height))
    };
    Rect::new(x, y, w, h)
}

fn fill_with(color: Color) -> Paint {
    Paint::new().color(color).style(PaintStyle::Fill)
}

fn intersect_rects(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

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

fn clip_paths_hash(paths: &[Path]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for path in paths {
        path.commands.len().hash(&mut hasher);
        hash_path(path, &mut hasher);
    }
    hasher.finish()
}

fn hash_image_quad(src: &Rect, quad: &[Point; 3], hasher: &mut impl Hasher) {
    for v in [src.x, src.y, src.width, src.height] {
        v.to_bits().hash(hasher);
    }
    for p in quad {
        p.x.to_bits().hash(hasher);
        p.y.to_bits().hash(hasher);
    }
}

impl<'list> RecordingContext<'list> {
    /// Creates a context that appends display items to the given display list.
    pub fn new(display_list: &'list mut DisplayList, width: u32, height: u32) -> Self {
        Self {
            display_list,
            next_index: 0,
            state_stack: SaveStack::new(),
            current_transform: Transform::identity(),
            current_opacity: 1.0,
            current_clip_rect: None,
            clip_paths: Vec::new(),
            current_clip_paths: None,
            current_clip_paths_hash: 0,
            path_clips: false,
            current_blend_mode: BlendMode::Normal,
            current_sampling: Sampling::Linear,
            current_pixel_snap: false,
            scale_factor: 1.0,
            current_interactive_id: None,
            width,
            height,
        }
    }

    /// Lets `clip_path` take any shape, for renderers that apply [`DisplayItem::clip_paths`].
    pub fn set_path_clips(&mut self, enabled: bool) {
        self.path_clips = enabled;
    }

    fn set_clip_paths_len(&mut self, len: usize) {
        if len != self.clip_paths.len() {
            self.clip_paths.truncate(len);
            self.sync_clip_paths();
        }
    }

    /// Rebuilds the shared clip stack and its hash after `clip_paths` changed.
    fn sync_clip_paths(&mut self) {
        self.current_clip_paths =
            (!self.clip_paths.is_empty()).then(|| Arc::from(self.clip_paths.as_slice()));
        self.current_clip_paths_hash = clip_paths_hash(&self.clip_paths);
    }

    /// Sets the scale factor used when computing cache keys (e.g. for HiDPI).
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale;
    }

    fn transform_is_axis_aligned(&self) -> bool {
        let t = self.current_transform;
        t.m12.abs() < 1e-6 && t.m21.abs() < 1e-6
    }

    fn ensure_image_transform(&self) -> AureaResult<()> {
        let t = self.current_transform;
        if [
            t.m11, t.m12, t.m13, t.m21, t.m22, t.m23, t.m31, t.m32, t.m33,
        ]
        .iter()
        .all(|v| v.is_finite())
            && t.m13.abs() < 1e-6
            && t.m23.abs() < 1e-6
            && (t.m33 - 1.0).abs() < 1e-6
        {
            Ok(())
        } else {
            Err(AureaError::Unsupported {
                operation: "non-affine image transform",
                platform: Platform::current(),
            })
        }
    }

    fn image_quad(&self, dest: Rect) -> [Point; 3] {
        [
            self.s_pt(Point::new(dest.x, dest.y)),
            self.s_pt(Point::new(dest.right(), dest.y)),
            self.s_pt(Point::new(dest.x, dest.bottom())),
        ]
    }

    fn upright_image(&self) -> bool {
        self.transform_is_axis_aligned()
            && self.current_transform.m11 >= 0.0
            && self.current_transform.m22 >= 0.0
    }

    fn transform_scale(&self) -> f32 {
        let t = self.current_transform;
        let det = t.m11 * t.m22 - t.m12 * t.m21;
        det.abs().sqrt()
    }

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

    fn s_pt(&self, p: Point) -> Point {
        let t = self.current_transform.map_point(p);
        Point::new(t.x * self.scale_factor, t.y * self.scale_factor)
    }

    fn s(&self, v: f32) -> f32 {
        v * self.transform_scale() * self.scale_factor
    }

    fn s_paint(&self, paint: &Paint) -> Paint {
        let mut p = paint.clone();
        let factor = self.transform_scale() * self.scale_factor;
        p.stroke_width *= factor;
        p.dash = p.dash.map(|dash| dash.scaled(factor));
        p
    }

    fn physical_path(&self, path: &Path) -> Path {
        let commands = path
            .commands
            .iter()
            .map(|cmd| match cmd {
                PathCommand::MoveTo(p) => PathCommand::MoveTo(self.s_pt(*p)),
                PathCommand::LineTo(p) => PathCommand::LineTo(self.s_pt(*p)),
                PathCommand::QuadTo(c, p) => PathCommand::QuadTo(self.s_pt(*c), self.s_pt(*p)),
                PathCommand::CubicTo(c1, c2, p) => {
                    PathCommand::CubicTo(self.s_pt(*c1), self.s_pt(*c2), self.s_pt(*p))
                }
                PathCommand::Close => PathCommand::Close,
            })
            .collect();
        Path {
            commands,
            fill_rule: path.fill_rule,
        }
    }

    fn transformed_rect_path(&self, r: Rect) -> Path {
        let corners = [
            self.s_pt(Point::new(r.x, r.y)),
            self.s_pt(Point::new(r.x + r.width, r.y)),
            self.s_pt(Point::new(r.x + r.width, r.y + r.height)),
            self.s_pt(Point::new(r.x, r.y + r.height)),
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
    pub fn set_interactive_id(&mut self, id: Option<InteractiveId>) {
        self.current_interactive_id = id;
    }

    /// Draws a rectangle and marks it as interactive with the given ID.
    pub fn draw_interactive_rect(
        &mut self,
        id: InteractiveId,
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
        id: InteractiveId,
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
        id: InteractiveId,
        path: &Path,
        paint: &Paint,
    ) -> AureaResult<()> {
        let old_id = self.current_interactive_id;
        self.current_interactive_id = Some(id);
        let result = self.draw_path(path, paint);
        self.current_interactive_id = old_id;
        result
    }

    fn compute_cache_key(&self, command: &DrawCommand) -> CacheKey {
        let mut hasher = DefaultHasher::new();
        match command {
            DrawCommand::Clear(color) => {
                "Clear".hash(&mut hasher);
                color.r.hash(&mut hasher);
                color.g.hash(&mut hasher);
                color.b.hash(&mut hasher);
                color.a.hash(&mut hasher);
            }
            DrawCommand::DrawRect(rect, paint) => {
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
            DrawCommand::DrawShadow(rect, radius, blur, color) => {
                "DrawShadow".hash(&mut hasher);
                for v in [rect.x, rect.y, rect.width, rect.height, *radius, *blur] {
                    v.to_bits().hash(&mut hasher);
                }
                [color.r, color.g, color.b, color.a].hash(&mut hasher);
            }
            DrawCommand::DrawRoundRect(rect, radius, paint) => {
                "DrawRoundRect".hash(&mut hasher);
                rect.x.to_bits().hash(&mut hasher);
                rect.y.to_bits().hash(&mut hasher);
                rect.width.to_bits().hash(&mut hasher);
                rect.height.to_bits().hash(&mut hasher);
                radius.to_bits().hash(&mut hasher);
                paint.color.r.hash(&mut hasher);
                paint.color.g.hash(&mut hasher);
                paint.color.b.hash(&mut hasher);
                paint.color.a.hash(&mut hasher);
                paint.style.hash(&mut hasher);
                paint.stroke_width.to_bits().hash(&mut hasher);
            }
            DrawCommand::DrawCircle(center, radius, paint) => {
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
            DrawCommand::DrawImageRect(image, dest, sampling) => {
                "DrawImageRect".hash(&mut hasher);
                sampling.hash(&mut hasher);
                image.width.hash(&mut hasher);
                image.height.hash(&mut hasher);
                dest.x.to_bits().hash(&mut hasher);
                dest.y.to_bits().hash(&mut hasher);
                dest.width.to_bits().hash(&mut hasher);
                dest.height.to_bits().hash(&mut hasher);
                image.id().hash(&mut hasher);
            }
            DrawCommand::DrawImageQuad(image, src, quad, sampling) => {
                "DrawImageQuad".hash(&mut hasher);
                image.id().hash(&mut hasher);
                sampling.hash(&mut hasher);
                hash_image_quad(src, quad, &mut hasher);
            }
            DrawCommand::DrawTextureQuad(texture, src, quad, sampling) => {
                "DrawTextureQuad".hash(&mut hasher);
                texture.hash(&mut hasher);
                sampling.hash(&mut hasher);
                hash_image_quad(src, quad, &mut hasher);
            }
            DrawCommand::DrawTexture(texture, src, dest, sampling) => {
                "DrawTexture".hash(&mut hasher);
                texture.hash(&mut hasher);
                sampling.hash(&mut hasher);
                for r in [src, dest] {
                    r.x.to_bits().hash(&mut hasher);
                    r.y.to_bits().hash(&mut hasher);
                    r.width.to_bits().hash(&mut hasher);
                    r.height.to_bits().hash(&mut hasher);
                }
            }
            DrawCommand::DrawImageRegion(image, src, dest, sampling) => {
                "DrawImageRegion".hash(&mut hasher);
                sampling.hash(&mut hasher);
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
                image.id().hash(&mut hasher);
            }
            DrawCommand::FillLinearGradient(grad, rect) => {
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
            DrawCommand::FillRadialGradient(grad, rect) => {
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
            DrawCommand::DrawPath(path, paint) => {
                "DrawPath".hash(&mut hasher);
                hash_path(path, &mut hasher);
                paint.color.r.hash(&mut hasher);
                paint.color.g.hash(&mut hasher);
                paint.color.b.hash(&mut hasher);
                paint.color.a.hash(&mut hasher);
                paint.style.hash(&mut hasher);
                paint.stroke_width.to_bits().hash(&mut hasher);
                paint.line_cap.hash(&mut hasher);
                paint.line_join.hash(&mut hasher);
                paint.miter_limit.to_bits().hash(&mut hasher);
                if let Some(dash) = &paint.dash {
                    dash.offset.to_bits().hash(&mut hasher);
                    for length in dash.lengths() {
                        length.to_bits().hash(&mut hasher);
                    }
                }
            }
            DrawCommand::DrawGlyphMask(mask, origin, color) => {
                "DrawGlyphMask".hash(&mut hasher);
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
        self.current_opacity.to_bits().hash(&mut hasher);
        self.scale_factor.to_bits().hash(&mut hasher);
        self.current_blend_mode.hash(&mut hasher);
        match self.current_clip_rect {
            Some(clip) => {
                1u8.hash(&mut hasher);
                clip.x.to_bits().hash(&mut hasher);
                clip.y.to_bits().hash(&mut hasher);
                clip.width.to_bits().hash(&mut hasher);
                clip.height.to_bits().hash(&mut hasher);
            }
            None => 0u8.hash(&mut hasher),
        }

        CacheKey::from_hash(hasher.finish())
    }

    fn compute_bounds(&self, command: &DrawCommand) -> Rect {
        match command {
            DrawCommand::Clear(_) => Rect::new(0.0, 0.0, f32::MAX, f32::MAX),
            DrawCommand::DrawRect(rect, paint) => {
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
            DrawCommand::DrawShadow(rect, _, blur, _) => {
                let reach = blur * 3.0;
                Rect::new(
                    rect.x - reach,
                    rect.y - reach,
                    rect.width + reach * 2.0,
                    rect.height + reach * 2.0,
                )
            }
            DrawCommand::DrawRoundRect(rect, _, paint) => {
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
            DrawCommand::DrawCircle(center, radius, paint) => {
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
            DrawCommand::DrawImageRect(_, dest, _)
            | DrawCommand::DrawImageRegion(_, _, dest, _)
            | DrawCommand::DrawTexture(_, _, dest, _) => *dest,
            DrawCommand::DrawImageQuad(_, _, quad, _)
            | DrawCommand::DrawTextureQuad(_, _, quad, _) => quad_bounds(quad),
            DrawCommand::DrawGlyphMask(mask, origin, _) => {
                Rect::new(origin.x, origin.y, mask.width as f32, mask.height as f32)
            }
            DrawCommand::FillLinearGradient(_, rect) => *rect,
            DrawCommand::FillRadialGradient(_, rect) => *rect,
            DrawCommand::DrawPath(path, paint) => {
                let mut bounds = path_bounds(path);
                if paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0 {
                    let reach = stroke_reach(paint);
                    bounds.x -= reach;
                    bounds.y -= reach;
                    bounds.width += 2.0 * reach;
                    bounds.height += 2.0 * reach;
                }
                bounds
            }
            _ => Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    /// A straight line drawn as a filled `rect`, snapped by [`snap_rule`] when snapping is on.
    fn draw_rule(&mut self, rect: Rect, color: Color) -> AureaResult<()> {
        if !self.current_pixel_snap || !self.transform_is_axis_aligned() {
            return self.draw_rect(rect, &fill_with(color));
        }
        let device = self.s_rect(rect);
        let rect = snap_rule(device, device.width >= device.height);
        self.add_command(DrawCommand::DrawRect(rect, fill_with(color)));
        Ok(())
    }

    fn is_opaque(&self, command: &DrawCommand) -> bool {
        match command {
            DrawCommand::Clear(color) => color.a == 255,
            DrawCommand::DrawRect(_, paint) => {
                paint.color.a == 255 && paint.style == PaintStyle::Fill
            }
            DrawCommand::DrawCircle(_, _, paint) => {
                paint.color.a == 255 && paint.style == PaintStyle::Fill
            }
            DrawCommand::DrawImageRect(image, ..) => image.is_opaque(),
            // A source reaching past the image leaves its edge pixels unwritten when shrinking.
            DrawCommand::DrawImageRegion(image, src, ..) => {
                src.width > 0.0
                    && src.height > 0.0
                    && src.x >= 0.0
                    && src.y >= 0.0
                    && src.right() <= image.width as f32
                    && src.bottom() <= image.height as f32
                    && image.is_opaque()
            }
            DrawCommand::DrawTexture(..)
            | DrawCommand::DrawImageQuad(..)
            | DrawCommand::DrawTextureQuad(..) => false,
            DrawCommand::FillLinearGradient(..) | DrawCommand::FillRadialGradient(..) => false,
            _ => false,
        }
    }

    fn add_command(&mut self, command: DrawCommand) {
        let mut cache_key = self.compute_cache_key(&command);
        if self.current_clip_paths.is_some() {
            let mut hasher = DefaultHasher::new();
            cache_key.0.hash(&mut hasher);
            self.current_clip_paths_hash.hash(&mut hasher);
            cache_key = CacheKey(hasher.finish());
        }
        let bounds = self.compute_bounds(&command);
        let opaque = self.is_opaque(&command)
            && self.current_opacity >= 1.0
            && self.current_clip_paths.is_none();

        let index = DisplayIndex(self.next_index);
        self.next_index += 1;

        let blend = self.current_blend_mode;
        let clip = self.current_clip_rect;
        let item = if let Some(interactive_id) = self.current_interactive_id {
            DisplayItem::new_interactive(
                index,
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
            DisplayItem::new(index, cache_key, bounds, opaque, blend, command)
                .with_clip(clip)
                .with_opacity(self.current_opacity)
        }
        .with_clip_paths(self.current_clip_paths.clone());

        self.display_list.push(item);
    }
}

impl DrawingContext for RecordingContext<'_> {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn clear(&mut self, color: Color) -> AureaResult<()> {
        self.add_command(DrawCommand::Clear(color));
        Ok(())
    }

    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, paint: &Paint) -> AureaResult<()> {
        if paint.style == PaintStyle::Stroke
            && paint.stroke_width > 0.0
            && !paint.needs_path_stroke()
        {
            let half = paint.stroke_width / 2.0;
            let horizontal = (y1 - y2).abs() < 1e-4;
            let vertical = (x1 - x2).abs() < 1e-4;

            if horizontal && !vertical {
                let rect = Rect::new(x1.min(x2), y1 - half, (x2 - x1).abs(), paint.stroke_width);
                return self.draw_rule(rect, paint.color);
            }
            if vertical && !horizontal {
                let rect = Rect::new(x1 - half, y1.min(y2), paint.stroke_width, (y2 - y1).abs());
                return self.draw_rule(rect, paint.color);
            }
        }

        let mut path = Path::new();
        path.commands.push(PathCommand::MoveTo(Point::new(x1, y1)));
        path.commands.push(PathCommand::LineTo(Point::new(x2, y2)));
        self.draw_path(&path, paint)
    }

    fn draw_round_rect(&mut self, rect: Rect, radius: f32, paint: &Paint) -> AureaResult<()> {
        if !self.transform_is_axis_aligned() || paint.dash.is_some() {
            return self.draw_path(&Path::rounded_rect(rect, radius), paint);
        }
        self.add_command(DrawCommand::DrawRoundRect(
            self.s_rect(rect),
            self.s(radius),
            self.s_paint(paint),
        ));
        Ok(())
    }
    fn draw_rect(&mut self, rect: Rect, paint: &Paint) -> AureaResult<()> {
        if paint.needs_path_stroke() && (paint.dash.is_some() || paint.line_join != LineJoin::Miter)
        {
            return self.draw_path(&rect_path(rect), paint);
        }
        if !self.transform_is_axis_aligned() {
            let path = self.transformed_rect_path(rect);
            self.add_command(DrawCommand::DrawPath(path, self.s_paint(paint)));
            return Ok(());
        }
        let (mut rect, mut paint) = (self.s_rect(rect), self.s_paint(paint));
        if self.current_pixel_snap {
            (rect, paint.stroke_width) = snap_rect(rect, &paint);
        }
        self.add_command(DrawCommand::DrawRect(rect, paint));
        Ok(())
    }

    fn draw_shadow(&mut self, rect: Rect, radius: f32, blur: f32, color: Color) -> AureaResult<()> {
        if color.a == 0 {
            return Ok(());
        }
        if !self.transform_is_axis_aligned() {
            return self.draw_path(
                &Path::rounded_rect(rect, radius),
                &Paint::new().color(color),
            );
        }
        self.add_command(DrawCommand::DrawShadow(
            self.s_rect(rect),
            self.s(radius),
            self.s(blur.max(0.0)),
            color,
        ));
        Ok(())
    }

    fn draw_circle(&mut self, center: Point, radius: f32, paint: &Paint) -> AureaResult<()> {
        if paint.style == PaintStyle::Stroke && paint.dash.is_some() {
            let mut path = Path::new();
            path.arc(center, radius, (0.0, TAU), false);
            path.commands.push(PathCommand::Close);
            return self.draw_path(&path, paint);
        }
        self.add_command(DrawCommand::DrawCircle(
            self.s_pt(center),
            self.s(radius),
            self.s_paint(paint),
        ));
        Ok(())
    }

    fn draw_path(&mut self, path: &Path, paint: &Paint) -> AureaResult<()> {
        self.add_command(DrawCommand::DrawPath(
            self.physical_path(path),
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
        self.draw_text_directed(text, point, font, paint, None)
    }

    fn draw_text_directed(
        &mut self,
        text: &str,
        point: Point,
        font: &Font,
        paint: &Paint,
        rtl: Option<bool>,
    ) -> AureaResult<()> {
        if text.is_empty() {
            return Ok(());
        }

        let physical_font = FontRef::with_size(font, self.s(font.size));
        let (mask, ascent, pad) = TEXT_RENDERER.render_text_directed(text, physical_font, rtl)?;
        if mask.width == 0 || mask.height == 0 {
            return Ok(());
        }

        let anchor = self.s_pt(point);
        let origin = Point::new((anchor.x - pad).round(), (anchor.y - ascent - pad).round());
        self.add_command(DrawCommand::DrawGlyphMask(mask, origin, paint.color));
        Ok(())
    }

    fn draw_image(&mut self, image: &Image, position: Point) -> AureaResult<()> {
        self.draw_image_rect(
            image,
            Rect::new(
                position.x,
                position.y,
                image.width as f32,
                image.height as f32,
            ),
        )
    }

    fn draw_image_rect(&mut self, image: &Image, dest: Rect) -> AureaResult<()> {
        self.ensure_image_transform()?;
        let command = if self.upright_image() {
            DrawCommand::DrawImageRect(image.clone(), self.s_rect(dest), self.current_sampling)
        } else {
            DrawCommand::DrawImageQuad(
                image.clone(),
                Rect::new(0.0, 0.0, image.width as f32, image.height as f32),
                self.image_quad(dest),
                self.current_sampling,
            )
        };
        self.add_command(command);
        Ok(())
    }

    fn draw_image_region(&mut self, image: &Image, src: Rect, dest: Rect) -> AureaResult<()> {
        self.ensure_image_transform()?;
        let command = if self.upright_image() {
            DrawCommand::DrawImageRegion(
                image.clone(),
                src,
                self.s_rect(dest),
                self.current_sampling,
            )
        } else {
            DrawCommand::DrawImageQuad(
                image.clone(),
                src,
                self.image_quad(dest),
                self.current_sampling,
            )
        };
        self.add_command(command);
        Ok(())
    }

    fn set_sampling(&mut self, sampling: Sampling) -> AureaResult<()> {
        self.current_sampling = sampling;
        Ok(())
    }

    fn set_pixel_snap(&mut self, snap: bool) -> AureaResult<()> {
        self.current_pixel_snap = snap;
        Ok(())
    }

    fn draw_texture(&mut self, texture: GpuTexture, src: Rect, dest: Rect) -> AureaResult<()> {
        self.ensure_image_transform()?;
        let command = if self.upright_image() {
            DrawCommand::DrawTexture(texture, src, self.s_rect(dest), self.current_sampling)
        } else {
            DrawCommand::DrawTextureQuad(texture, src, self.image_quad(dest), self.current_sampling)
        };
        self.add_command(command);
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
        let sf = self.scale_factor;
        let physical_font = FontRef::with_size(font, font.size * sf);
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
        self.state_stack.push(DrawingState {
            transform: self.current_transform,
            opacity: self.current_opacity,
            clip_rect: self.current_clip_rect,
            clip_paths: self.clip_paths.len(),
            blend_mode: self.current_blend_mode,
            sampling: self.current_sampling,
            pixel_snap: self.current_pixel_snap,
        });
        Ok(())
    }

    fn restore(&mut self) -> AureaResult<()> {
        if let Some(state) = self.state_stack.pop() {
            self.current_transform = state.transform;
            self.current_opacity = state.opacity;
            self.current_clip_rect = state.clip_rect;
            self.set_clip_paths_len(state.clip_paths);
            self.current_blend_mode = state.blend_mode;
            self.current_sampling = state.sampling;
            self.current_pixel_snap = state.pixel_snap;
        }
        Ok(())
    }

    fn transform(&mut self, transform: Transform) -> AureaResult<()> {
        self.current_transform = transform.multiply(self.current_transform);
        Ok(())
    }

    fn clip_rect(&mut self, rect: Rect) -> AureaResult<()> {
        let r = self.s_rect(rect);
        self.current_clip_rect = Some(match self.current_clip_rect {
            Some(current) => intersect_rects(current, r),
            None => r,
        });
        Ok(())
    }

    fn clip_path(&mut self, path: &Path) -> AureaResult<()> {
        let physical = self.physical_path(path);
        let rect = match axis_aligned_rect(&physical) {
            Some(rect) => rect,
            None if self.path_clips => {
                let b = path_bounds(&physical);
                let (x, y) = (b.x.floor(), b.y.floor());
                let rect = Rect::new(
                    x,
                    y,
                    (b.x + b.width).ceil() - x,
                    (b.y + b.height).ceil() - y,
                );
                self.clip_paths.push(physical);
                self.sync_clip_paths();
                rect
            }
            None => {
                return Err(AureaError::Unsupported {
                    operation: "clip_path with a non-rectangular path",
                    platform: Platform::current(),
                });
            }
        };

        self.current_clip_rect = Some(match self.current_clip_rect {
            Some(current) => intersect_rects(current, rect),
            None => rect,
        });
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
        let mut g = gradient.clone();
        g.start = self.s_pt(g.start);
        g.end = self.s_pt(g.end);
        self.add_command(DrawCommand::FillLinearGradient(g, self.s_rect(rect)));
        Ok(())
    }

    fn fill_radial_gradient(&mut self, gradient: &RadialGradient, rect: Rect) -> AureaResult<()> {
        let mut g = gradient.clone();
        g.center = self.s_pt(g.center);
        g.radius = self.s(g.radius);
        self.add_command(DrawCommand::FillRadialGradient(g, self.s_rect(rect)));
        Ok(())
    }

    fn hit_test_path(&mut self, path: &Path, point: Point) -> AureaResult<bool> {
        Ok(hit_test_path(path, point))
    }
}

#[cfg(test)]
mod tests {
    use std::thread::spawn;

    #[test]
    fn measuring_off_the_ui_thread_matches_drawing_widths() {
        let font = Font::new("", 16.0);
        let here = measure_text("Hello, measured world", &font).expect("measure");
        let there = spawn(move || measure_text("Hello, measured world", &Font::new("", 16.0)))
            .join()
            .expect("thread")
            .expect("measure");
        assert!(here.width > 0.0);
        assert_eq!(here.width, there.width);
        assert!(measure_text("ab", &font).expect("measure").width < here.width);
    }

    use super::*;

    fn ctx_with(list: &mut DisplayList, scale: f32) -> RecordingContext<'_> {
        let mut ctx = RecordingContext::new(list, 100, 100);
        ctx.set_scale_factor(scale);
        ctx
    }

    #[test]
    fn fractional_dpi_text_is_rasterized_at_device_size_and_pixel_aligned() {
        let font = Font::new("", 14.0);
        let mut heights = Vec::new();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut list = DisplayList::new();
            {
                let mut ctx = ctx_with(&mut list, scale);
                ctx.draw_text_with_font("Hello", Point::new(4.25, 30.75), &font, &Paint::default())
                    .expect("text");
            }
            let DrawCommand::DrawGlyphMask(mask, origin, _) = &list.items()[0].command else {
                panic!("glyph mask")
            };
            assert_eq!(origin.x, origin.x.round());
            assert_eq!(origin.y, origin.y.round());
            heights.push(mask.height);
        }
        assert!(
            heights[3] > heights[0],
            "HiDPI text must have more raster pixels"
        );
    }

    fn snapped_rects(rects: &[Rect]) -> Vec<Rect> {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            ctx.set_pixel_snap(true).expect("snap");
            for &rect in rects {
                ctx.draw_rect(rect, &Paint::default()).expect("draw_rect");
            }
        }
        list.items()
            .iter()
            .map(|item| match &item.command {
                DrawCommand::DrawRect(rect, _) => *rect,
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    #[test]
    fn touching_snapped_rects_share_an_edge() {
        let out = snapped_rects(&[Rect::new(0.4, 0.0, 3.4, 1.0), Rect::new(3.8, 0.0, 2.0, 1.0)]);
        let (a, b) = (out[0], out[1]);
        assert_eq!(a.x + a.width, b.x, "no gap, no overlap: {a:?} {b:?}");
    }

    #[test]
    fn snapped_fill_edges_round_independently() {
        let out = snapped_rects(&[Rect::new(4.3, 0.0, 6.4, 1.0)]);
        assert_eq!(out[0].x, 4.0);
        assert_eq!(out[0].x + out[0].width, 11.0);
    }

    #[test]
    fn a_context_cannot_outlive_its_renderer() {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            ctx.draw_rect(Rect::new(0.0, 0.0, 1.0, 1.0), &Paint::default())
                .expect("draw_rect");
        }
        assert_eq!(list.items().len(), 1);
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
    fn transforms_landing_in_the_same_place_share_a_key() {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            ctx.draw_rect(Rect::new(10.0, 10.0, 5.0, 5.0), &Paint::default())
                .expect("draw_rect");
            ctx.translate(10.0, 10.0).expect("translate");
            ctx.draw_rect(Rect::new(0.0, 0.0, 5.0, 5.0), &Paint::default())
                .expect("draw_rect");
        }

        let items = list.items();
        assert_eq!(
            items[0].cache_key, items[1].cache_key,
            "same pixels, so the damage diff should see no change"
        );
        assert_eq!(items[0].bounds, items[1].bounds);
    }

    #[test]
    fn a_transform_that_moves_a_shape_changes_its_key() {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            ctx.draw_rect(Rect::new(0.0, 0.0, 5.0, 5.0), &Paint::default())
                .expect("draw_rect");
            ctx.translate(7.0, 0.0).expect("translate");
            ctx.draw_rect(Rect::new(0.0, 0.0, 5.0, 5.0), &Paint::default())
                .expect("draw_rect");
        }

        let items = list.items();
        assert_ne!(items[0].cache_key, items[1].cache_key);
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

    fn clipped_keys(clip: &Path, rects: usize) -> Vec<CacheKey> {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            ctx.set_path_clips(true);
            ctx.clip_path(clip).expect("clip_path");
            for _ in 0..rects {
                ctx.draw_rect(Rect::new(10.0, 10.0, 5.0, 5.0), &Paint::default())
                    .expect("draw_rect");
            }
        }
        list.items().iter().map(|item| item.cache_key).collect()
    }

    #[test]
    fn items_under_one_clip_share_keys_across_recordings() {
        let clip = polygon_clip(12, 50.0, 50.0, 40.0);
        let first = clipped_keys(&clip, 2);
        assert_eq!(first[0], first[1], "same pixels under the same clip");
        assert_eq!(first, clipped_keys(&clip, 2));
    }

    #[test]
    fn different_clip_paths_change_cache_keys() {
        let a = polygon_clip(12, 50.0, 50.0, 40.0);
        let b = polygon_clip(12, 51.0, 50.0, 40.0);
        assert_ne!(clipped_keys(&a, 1)[0], clipped_keys(&b, 1)[0]);
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            ctx.draw_rect(Rect::new(10.0, 10.0, 5.0, 5.0), &Paint::default())
                .expect("draw_rect");
        }
        assert_ne!(clipped_keys(&a, 1)[0], list.items()[0].cache_key);
    }

    #[test]
    fn restoring_past_a_clip_path_restores_unclipped_keys() {
        let clip = polygon_clip(12, 50.0, 50.0, 40.0);
        let rect = Rect::new(10.0, 10.0, 5.0, 5.0);
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            ctx.set_path_clips(true);
            ctx.save().expect("save");
            ctx.clip_path(&clip).expect("clip_path");
            ctx.draw_rect(rect, &Paint::default()).expect("draw_rect");
            ctx.restore().expect("restore");
            ctx.draw_rect(rect, &Paint::default()).expect("draw_rect");
        }
        let mut plain = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut plain, 1.0);
            ctx.draw_rect(rect, &Paint::default()).expect("draw_rect");
        }
        let items = list.items();
        assert_ne!(items[0].cache_key, plain.items()[0].cache_key);
        assert_eq!(items[1].cache_key, plain.items()[0].cache_key);
    }

    fn polygon_clip(points: usize, cx: f32, cy: f32, radius: f32) -> Path {
        use std::f32::consts::TAU;

        let mut path = Path::new();
        for i in 0..points {
            let angle = i as f32 / points as f32 * TAU;
            let p = Point::new(cx + radius * angle.cos(), cy + radius * angle.sin());
            path.commands.push(if i == 0 {
                PathCommand::MoveTo(p)
            } else {
                PathCommand::LineTo(p)
            });
        }
        path.commands.push(PathCommand::Close);
        path
    }

    #[test]
    #[ignore]
    fn clipped_recording_frame_time() {
        use std::time::Instant;

        let clip = polygon_clip(64, 50.0, 50.0, 40.0);
        let paint = Paint::default();
        let frames = 50u32;
        let start = Instant::now();
        for _ in 0..frames {
            let mut list = DisplayList::new();
            {
                let mut ctx = RecordingContext::new(&mut list, 100, 100);
                ctx.set_path_clips(true);
                ctx.clip_path(&clip).expect("clip_path");
                for i in 0..2000 {
                    let rect = Rect::new((i % 50) as f32, (i / 50) as f32, 4.0, 4.0);
                    ctx.draw_rect(rect, &paint).expect("draw_rect");
                }
            }
            assert_eq!(list.items().len(), 2000);
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
        println!("clipped recording: {ms:.3} ms/frame");
    }
}
