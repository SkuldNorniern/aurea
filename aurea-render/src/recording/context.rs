//! Records draw calls into a display list.

mod geom;
mod key;

use crate::effect::Effect;
use crate::renderer::DrawingContext;
use crate::scene::command::DrawCommand;
use crate::scene::display_list::{DisplayIndex, DisplayItem, DisplayList};
use crate::scene::hit_test::{hit_test_path, path_bounds};
use crate::text::{
    FaceData, FaceId, FontRef, GlyphCoverage, LayoutOptions, ShapedText, TextLayout, TextRenderer,
};
use crate::types::*;
use aurea_foundation::{AureaError, AureaResult, Platform};
use geom::{
    axis_aligned_rect, command_bounds, corners, intersect_rects, quad_path, snap_rect, snap_rule,
};
use key::{clip_paths_hash, command_key, under_clip};
use std::f32::consts::TAU;
use std::sync::{Arc, LazyLock};
use std::thread::{self, JoinHandle};

static TEXT_RENDERER: LazyLock<TextRenderer> = LazyLock::new(TextRenderer::new);
static DEFAULT_FONT: LazyLock<Font> =
    LazyLock::new(|| Font::new(DEFAULT_FONT_FAMILY, DEFAULT_FONT_SIZE));
const DEFAULT_FONT_FAMILY: &str = "Sans";
const DEFAULT_FONT_SIZE: f32 = 16.0;
const NO_TEXT: TextMetrics = TextMetrics {
    width: 0.0,
    height: 0.0,
    ascent: 0.0,
    descent: 0.0,
    advance: 0.0,
};

/// Measures `text` in `font` at logical size, with the same text engine and caches the CPU renderer
/// draws with, so what is measured is what is drawn.
pub fn measure_text(text: &str, font: &Font) -> AureaResult<TextMetrics> {
    if text.is_empty() {
        return Ok(NO_TEXT);
    }
    TEXT_RENDERER.measure_text(text, FontRef::with_size(font, font.size))
}

/// Rasterizes the glyphs of `text` in `font` at each of `scales` on a thread of its own, so the
/// first frame using them needn't: for a language's label set at start, or a new display scale.
pub fn warm_text(font: &Font, text: &str, scales: &[f32]) -> JoinHandle<()> {
    let (font, text, scales) = (font.clone(), text.to_string(), scales.to_vec());
    thread::spawn(move || {
        let mut one = String::new();
        for scale in scales {
            let sized = FontRef::with_size(&font, font.size * scale);
            for ch in text.chars() {
                one.clear();
                one.push(ch);
                if let Err(error) = TEXT_RENDERER.render_text_directed(&one, sized, None) {
                    log::debug!("warming {ch:?}: {error}");
                }
            }
        }
    })
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

/// The rows a glyph paints as visible ink at `size`: the first, down from the baseline, and how
/// many. `None` for a glyph with no ink.
pub fn glyph_ink_rows(face: FaceId, glyph: u16, size: f32) -> AureaResult<Option<(i32, u32)>> {
    TEXT_RENDERER.glyph_ink_rows(face, glyph, size)
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

/// Drawing state that `save` and `restore` cover.
#[derive(Clone, Copy)]
struct State {
    transform: Transform,
    opacity: f32,
    clip_rect: Option<Rect>,
    blend_mode: BlendMode,
    sampling: Sampling,
    pixel_snap: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            transform: Transform::identity(),
            opacity: 1.0,
            clip_rect: None,
            blend_mode: BlendMode::Normal,
            sampling: Sampling::Linear,
            pixel_snap: false,
        }
    }
}

/// A `save`: the state, and how many clip paths were in force.
#[derive(Clone, Copy)]
struct Saved {
    state: State,
    clip_paths: usize,
}

const INLINE_SAVES: usize = 32;

/// Saves without allocating up to `INLINE_SAVES` deep; deeper ones spill to the heap.
struct SaveStack {
    inline: [Option<Saved>; INLINE_SAVES],
    len: usize,
    spill: Vec<Saved>,
}

impl SaveStack {
    const fn new() -> Self {
        Self {
            inline: [None; INLINE_SAVES],
            len: 0,
            spill: Vec::new(),
        }
    }

    fn push(&mut self, saved: Saved) {
        match self.inline.get_mut(self.len) {
            Some(slot) => {
                *slot = Some(saved);
                self.len += 1;
            }
            None => self.spill.push(saved),
        }
    }

    fn pop(&mut self) -> Option<Saved> {
        if let Some(saved) = self.spill.pop() {
            return Some(saved);
        }
        self.len = self.len.checked_sub(1)?;
        self.inline[self.len].take()
    }
}

/// Clip paths in force, with the shared copy and hash that items under them carry.
#[derive(Default)]
struct ClipPaths {
    paths: Vec<Path>,
    shared: Option<Arc<[Path]>>,
    hash: u64,
}

impl ClipPaths {
    fn push(&mut self, path: Path) {
        self.paths.push(path);
        self.sync();
    }

    fn truncate(&mut self, len: usize) {
        if len != self.paths.len() {
            self.paths.truncate(len);
            self.sync();
        }
    }

    /// Rebuilds the shared copy and hash after `paths` changed.
    fn sync(&mut self) {
        self.shared = if self.paths.is_empty() {
            None
        } else {
            Some(Arc::from(self.paths.as_slice()))
        };
        self.hash = clip_paths_hash(&self.paths);
    }
}

/// Whether `t` is affine with no shear or perspective, so images can be placed as rectangles.
fn is_upright_affine(t: Transform) -> bool {
    [
        t.m11, t.m12, t.m13, t.m21, t.m22, t.m23, t.m31, t.m32, t.m33,
    ]
    .iter()
    .all(|v| v.is_finite())
        && t.m13.abs() < 1e-6
        && t.m23.abs() < 1e-6
        && (t.m33 - 1.0).abs() < 1e-6
}

fn fill_with(color: Color) -> Paint {
    Paint::new().color(color).style(PaintStyle::Fill)
}

/// Whether `command` covers every pixel of its box with opaque paint.
fn is_opaque(command: &DrawCommand) -> bool {
    match command {
        DrawCommand::Clear(color) => color.a == 255,
        DrawCommand::DrawRect(_, paint) => paint.color.a == 255 && paint.style == PaintStyle::Fill,
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
        _ => false,
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
    state: State,
    saves: SaveStack,
    clips: ClipPaths,
    path_clips: bool,
    scale_factor: f32,
    interactive: Option<InteractiveId>,
    width: u32,
    height: u32,
}

impl<'list> RecordingContext<'list> {
    /// Creates a context that appends display items to the given display list.
    pub fn new(display_list: &'list mut DisplayList, width: u32, height: u32) -> Self {
        Self {
            display_list,
            next_index: 0,
            state: State::default(),
            saves: SaveStack::new(),
            clips: ClipPaths::default(),
            path_clips: false,
            scale_factor: 1.0,
            interactive: None,
            width,
            height,
        }
    }

    /// Lets `clip_path` take any shape, for renderers that apply [`DisplayItem::clip_paths`].
    pub fn set_path_clips(&mut self, enabled: bool) {
        self.path_clips = enabled;
    }

    /// Sets the scale factor used when computing cache keys (e.g. for HiDPI).
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale;
    }

    /// Sets the interactive ID for the next drawn shapes (used for hit testing).
    pub fn set_interactive_id(&mut self, id: Option<InteractiveId>) {
        self.interactive = id;
    }

    /// Draws a rectangle and marks it as interactive with the given ID.
    pub fn draw_interactive_rect(
        &mut self,
        id: InteractiveId,
        rect: Rect,
        paint: &Paint,
    ) -> AureaResult<()> {
        let old = self.interactive.replace(id);
        let result = self.draw_rect(rect, paint);
        self.interactive = old;
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
        let old = self.interactive.replace(id);
        let result = self.draw_circle(center, radius, paint);
        self.interactive = old;
        result
    }

    /// Draws a path and marks it as interactive with the given ID.
    pub fn draw_interactive_path(
        &mut self,
        id: InteractiveId,
        path: &Path,
        paint: &Paint,
    ) -> AureaResult<()> {
        let old = self.interactive.replace(id);
        let result = self.draw_path(path, paint);
        self.interactive = old;
        result
    }

    fn transform_is_axis_aligned(&self) -> bool {
        let t = self.state.transform;
        t.m12.abs() < 1e-6 && t.m21.abs() < 1e-6
    }

    fn ensure_image_transform(&self) -> AureaResult<()> {
        if is_upright_affine(self.state.transform) {
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
            && self.state.transform.m11 >= 0.0
            && self.state.transform.m22 >= 0.0
    }

    fn transform_scale(&self) -> f32 {
        let t = self.state.transform;
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
        let t = self.state.transform.map_point(p);
        Point::new(t.x * self.scale_factor, t.y * self.scale_factor)
    }

    /// The four corners of `r` in physical pixels.
    fn s_corners(&self, r: Rect) -> [Point; 4] {
        let c = corners(r);
        [
            self.s_pt(c[0]),
            self.s_pt(c[1]),
            self.s_pt(c[2]),
            self.s_pt(c[3]),
        ]
    }

    fn s(&self, v: f32) -> f32 {
        v * self.transform_scale() * self.scale_factor
    }

    fn s_paint(&self, paint: &Paint) -> Paint {
        let mut p = paint.clone();
        let factor = self.transform_scale() * self.scale_factor;
        p.stroke_width *= factor;
        if let Some(dash) = p.dash {
            p.dash = Some(dash.scaled(factor));
        }
        p
    }

    fn s_command(&self, cmd: &PathCommand) -> PathCommand {
        match cmd {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(self.s_pt(*p)),
            PathCommand::LineTo(p) => PathCommand::LineTo(self.s_pt(*p)),
            PathCommand::QuadTo(c, p) => PathCommand::QuadTo(self.s_pt(*c), self.s_pt(*p)),
            PathCommand::CubicTo(c1, c2, p) => {
                PathCommand::CubicTo(self.s_pt(*c1), self.s_pt(*c2), self.s_pt(*p))
            }
            PathCommand::Close => PathCommand::Close,
        }
    }

    /// `path` in physical pixels, reusing the display list's command storage.
    fn physical_path(&mut self, path: &Path) -> Path {
        let mut commands = self.display_list.take_commands();
        for cmd in &path.commands {
            commands.push(self.s_command(cmd));
        }
        Path {
            commands,
            fill_rule: path.fill_rule,
        }
    }

    /// Narrows the clip rect to `rect`, which is in physical pixels.
    fn narrow_clip(&mut self, rect: Rect) {
        self.state.clip_rect = Some(match self.state.clip_rect {
            Some(current) => intersect_rects(current, rect),
            None => rect,
        });
    }

    /// Records `command` as one display item under the current state and clip paths.
    fn add_command(&mut self, command: DrawCommand) {
        let mut cache_key = command_key(&command, &self.state, self.scale_factor);
        if self.clips.shared.is_some() {
            cache_key = under_clip(cache_key, self.clips.hash);
        }
        let bounds = command_bounds(&command);
        let opaque =
            is_opaque(&command) && self.state.opacity >= 1.0 && self.clips.shared.is_none();

        let index = DisplayIndex(self.next_index);
        self.next_index += 1;

        let blend = self.state.blend_mode;
        let item = match self.interactive {
            Some(id) => {
                DisplayItem::new_interactive(index, cache_key, bounds, opaque, id, blend, command)
            }
            None => DisplayItem::new(index, cache_key, bounds, opaque, blend, command),
        }
        .with_clip(self.state.clip_rect)
        .with_opacity(self.state.opacity)
        .with_clip_paths(self.clips.shared.clone());

        self.display_list.push(item);
    }

    /// A straight line drawn as a filled `rect`, snapped by [`snap_rule`] when snapping is on.
    fn draw_rule(&mut self, rect: Rect, color: Color) -> AureaResult<()> {
        if !self.state.pixel_snap || !self.transform_is_axis_aligned() {
            return self.draw_rect(rect, &fill_with(color));
        }
        let device = self.s_rect(rect);
        let rect = snap_rule(device, device.width >= device.height);
        self.add_command(DrawCommand::DrawRect(rect, fill_with(color)));
        Ok(())
    }
}

impl DrawingContext for RecordingContext<'_> {
    fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

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
        // filling a line's zero area drew a hairline whole and leaked bars along a clip edge
        let stroke = paint.clone().style(PaintStyle::Stroke);
        let paint = &stroke;
        if paint.stroke_width > 0.0 && !paint.needs_path_stroke() {
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
            return self.draw_path(&quad_path(corners(rect)), paint);
        }
        if !self.transform_is_axis_aligned() {
            let path = quad_path(self.s_corners(rect));
            self.add_command(DrawCommand::DrawPath(path, self.s_paint(paint)));
            return Ok(());
        }
        let (mut rect, mut paint) = (self.s_rect(rect), self.s_paint(paint));
        if self.state.pixel_snap {
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

    fn backdrop(&mut self, shape: &Path, effects: &[Effect]) -> AureaResult<()> {
        let Some(bounds) = shape.bounds() else {
            return Ok(());
        };
        if effects.is_empty() {
            return Ok(());
        }
        let mut scaled = Vec::with_capacity(effects.len());
        for effect in effects {
            scaled.push(match effect {
                Effect::Blur(radius) => Effect::Blur(self.s(*radius)),
                other => other.clone(),
            });
        }
        let effects: Arc<[Effect]> = scaled.into();
        self.save()?;
        let recorded = self.clip_path(shape).map(|()| {
            self.add_command(DrawCommand::Backdrop(self.s_rect(bounds), effects));
        });
        self.restore()?;
        recorded
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

    fn draw_columns(
        &mut self,
        x: f32,
        width: f32,
        spans: &[(f32, f32)],
        color: Color,
    ) -> AureaResult<()> {
        if spans.is_empty() {
            return Ok(());
        }
        if !self.transform_is_axis_aligned() {
            let paint = Paint::new().color(color);
            let mut left = x;
            for (top, bottom) in spans {
                self.draw_rect(Rect::new(left, *top, width, bottom - top), &paint)?;
                left += width;
            }
            return Ok(());
        }
        let start = self.s_pt(Point::new(x, 0.0)).x;
        let wide = self.s_pt(Point::new(x + width, 0.0)).x - start;
        let mut physical = Vec::with_capacity(spans.len());
        for &(top, bottom) in spans {
            let a = self.s_pt(Point::new(x, top)).y;
            let b = self.s_pt(Point::new(x, bottom)).y;
            physical.push((a.min(b), a.max(b)));
        }
        self.add_command(DrawCommand::DrawColumns(start, wide, physical, color));
        Ok(())
    }

    fn draw_path(&mut self, path: &Path, paint: &Paint) -> AureaResult<()> {
        let physical = self.physical_path(path);
        self.add_command(DrawCommand::DrawPath(physical, self.s_paint(paint)));
        Ok(())
    }

    fn draw_text(&mut self, text: &str, point: Point, paint: &Paint) -> AureaResult<()> {
        self.draw_text_with_font(text, point, &DEFAULT_FONT, paint)
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
            DrawCommand::DrawImageRect(image.clone(), self.s_rect(dest), self.state.sampling)
        } else {
            DrawCommand::DrawImageQuad(
                image.clone(),
                Rect::new(0.0, 0.0, image.width as f32, image.height as f32),
                self.image_quad(dest),
                self.state.sampling,
            )
        };
        self.add_command(command);
        Ok(())
    }

    fn draw_image_region(&mut self, image: &Image, src: Rect, dest: Rect) -> AureaResult<()> {
        self.ensure_image_transform()?;
        let command = if self.upright_image() {
            DrawCommand::DrawImageRegion(image.clone(), src, self.s_rect(dest), self.state.sampling)
        } else {
            DrawCommand::DrawImageQuad(
                image.clone(),
                src,
                self.image_quad(dest),
                self.state.sampling,
            )
        };
        self.add_command(command);
        Ok(())
    }

    fn set_sampling(&mut self, sampling: Sampling) -> AureaResult<()> {
        self.state.sampling = sampling;
        Ok(())
    }

    fn set_pixel_snap(&mut self, snap: bool) -> AureaResult<()> {
        self.state.pixel_snap = snap;
        Ok(())
    }

    fn draw_texture(&mut self, texture: GpuTexture, src: Rect, dest: Rect) -> AureaResult<()> {
        self.ensure_image_transform()?;
        let command = if self.upright_image() {
            DrawCommand::DrawTexture(texture, src, self.s_rect(dest), self.state.sampling)
        } else {
            DrawCommand::DrawTextureQuad(texture, src, self.image_quad(dest), self.state.sampling)
        };
        self.add_command(command);
        Ok(())
    }

    fn measure_text(&mut self, text: &str, font: &Font) -> AureaResult<TextMetrics> {
        if text.is_empty() {
            return Ok(NO_TEXT);
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
        self.saves.push(Saved {
            state: self.state,
            clip_paths: self.clips.paths.len(),
        });
        Ok(())
    }

    fn restore(&mut self) -> AureaResult<()> {
        if let Some(saved) = self.saves.pop() {
            self.state = saved.state;
            self.clips.truncate(saved.clip_paths);
        }
        Ok(())
    }

    fn transform(&mut self, transform: Transform) -> AureaResult<()> {
        self.state.transform = transform.multiply(self.state.transform);
        Ok(())
    }

    fn clip_rect(&mut self, rect: Rect) -> AureaResult<()> {
        let r = self.s_rect(rect);
        self.narrow_clip(r);
        Ok(())
    }

    fn clip_path(&mut self, path: &Path) -> AureaResult<()> {
        let physical = self.physical_path(path);
        if let Some(rect) = axis_aligned_rect(&physical) {
            self.narrow_clip(rect);
            return Ok(());
        }
        if !self.path_clips {
            return Err(AureaError::Unsupported {
                operation: "clip_path with a non-rectangular path",
                platform: Platform::current(),
            });
        }
        let b = path_bounds(&physical);
        let (x, y) = (b.x.floor(), b.y.floor());
        let rect = Rect::new(
            x,
            y,
            (b.x + b.width).ceil() - x,
            (b.y + b.height).ceil() - y,
        );
        self.clips.push(physical);
        self.narrow_clip(rect);
        Ok(())
    }

    fn set_alpha(&mut self, alpha: f32) -> AureaResult<()> {
        self.state.opacity = alpha;
        Ok(())
    }

    fn set_blend_mode(&mut self, mode: BlendMode) -> AureaResult<()> {
        self.state.blend_mode = mode;
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
    #[test]
    fn warming_fills_the_glyph_cache_off_the_ui_thread() {
        use super::{Font, FontRef, TEXT_RENDERER, warm_text};
        let font = Font::new("", 19.0);
        warm_text(&font, "warm ?쒓?", &[1.0, 2.0])
            .join()
            .expect("warm");
        let again = TEXT_RENDERER.render_text_directed("w", FontRef::with_size(&font, 38.0), None);
        assert!(again.is_ok());
    }

    use std::thread::spawn;

    #[test]
    fn set_size_changes_only_what_is_reported() {
        let mut list = DisplayList::new();
        let mut ctx = RecordingContext::new(&mut list, 400, 300);
        ctx.set_size(360, 250);
        assert_eq!((ctx.width(), ctx.height()), (360, 250));
    }

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
    use crate::scene::display_list::CacheKey;

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

    /// Keys of the items drawn by `record`, which runs against a fresh context at scale 1.
    fn keys_of(record: impl FnOnce(&mut RecordingContext<'_>) -> AureaResult<()>) -> Vec<CacheKey> {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            record(&mut ctx).expect("record");
        }
        list.items().iter().map(|item| item.cache_key).collect()
    }

    #[test]
    fn a_path_with_another_fill_rule_has_another_key() {
        let nonzero = polygon_clip(12, 50.0, 50.0, 40.0);
        let even_odd = Path {
            fill_rule: FillRule::EvenOdd,
            ..nonzero.clone()
        };
        let keys = keys_of(|ctx| {
            ctx.draw_path(&nonzero, &Paint::default())?;
            ctx.draw_path(&even_odd, &Paint::default())
        });
        assert_ne!(keys[0], keys[1]);
    }

    #[test]
    fn stroke_joins_and_miters_change_round_rect_keys() {
        let rect = Rect::new(10.0, 10.0, 20.0, 20.0);
        let miter = Paint::new().style(PaintStyle::Stroke);
        let round = miter.clone().line_join(LineJoin::Round);
        let limited = miter.clone().miter_limit(2.0);
        let keys = keys_of(|ctx| {
            ctx.draw_round_rect(rect, 4.0, &miter)?;
            ctx.draw_round_rect(rect, 4.0, &round)?;
            ctx.draw_round_rect(rect, 4.0, &limited)
        });
        assert_ne!(keys[0], keys[1], "joins");
        assert_ne!(keys[0], keys[2], "miter limits");
    }

    #[test]
    fn empty_columns_record_nothing() {
        let mut list = DisplayList::new();
        {
            let mut ctx = ctx_with(&mut list, 1.0);
            ctx.draw_columns(0.0, 4.0, &[], Color::rgb(0, 0, 0))
                .expect("columns");
        }
        assert!(list.items().is_empty());
    }

    #[test]
    fn items_under_one_clip_share_keys_across_recordings() {
        let clip = polygon_clip(12, 50.0, 50.0, 40.0);
        let first = clipped_keys(&clip, 2);
        assert_eq!(first[0], first[1], "same pixels under the same clip");
        assert_eq!(first, clipped_keys(&clip, 2));
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
}
