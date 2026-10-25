//! CPU rasterizer — flat framebuffer, no tile overhead.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem::{self, size_of_val};
use std::slice::from_raw_parts;
use std::sync::Arc;

use crate::cpu::blend::{ConstSrc, LinearLuts, blend_pixel};
use crate::cpu::clip::ClipBox;
#[cfg(test)]
use crate::cpu::damage::*;
use crate::cpu::damage::{
    FrameDamage, is_known_bounds, mark_occluded, resolve_frame_damage, should_render_item,
    uncovered_runs, union_rect,
};
use crate::cpu::raster::fill::{self, PathScratch};
use crate::cpu::raster::sample;
use crate::cpu::stroke;
#[cfg(test)]
use crate::cpu::tiles::*;
use crate::cpu::tiles::{
    mark_tile_range_dirty, tile_grid_dims, tile_range, union_dirty_tile_rects,
};
use crate::memory::smaller_buffer;
use crate::numeric::{
    f32_to_i32_clamped, f32_to_u8_clamped, f32_to_u32_clamped, f32_to_usize_clamped,
};
use crate::recording::RecordingContext;
use crate::renderer::{DrawingContext, FrameOutcome, FrameOutput, Renderer};
use crate::scene::command::{DrawCommand, quad_bounds};
use crate::scene::display_list::{CacheKey, DisplayItem, DisplayList};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::{
    BlendMode, Color, GlyphMask, GradientStop, Image, LinearGradient, Paint, PaintStyle, Path,
    PathCommand, Point, RadialGradient, Rect, Sampling,
};
use aurea_foundation::{AureaError, AureaResult};

/// Start and multiplier of the per-tile fold of item hashes in draw order.
const TILE_HASH_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
const TILE_HASH_MUL: u64 = 0xBF58_476D_1CE4_E5B9;

/// An item's cache key and bounds as one hash, for the tile fold.
fn item_hash(item: &DisplayItem) -> u64 {
    let mut h = DefaultHasher::new();
    item.cache_key.0.hash(&mut h);
    let b = item.bounds;
    for v in [b.x, b.y, b.width, b.height] {
        v.to_bits().hash(&mut h);
    }
    h.finish()
}

/// CPU software rasterizer drawing into a frame buffer.
pub struct CpuRasterizer {
    frame_buffer: Vec<u32>,
    width: u32,
    height: u32,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
    display_list: DisplayList,
    pending_damage: Option<Rect>,
    prev_items: Vec<(CacheKey, Rect)>,
    tile_hashes: Vec<u64>,
    scratch_path: PathScratch,
    scratch_row: ImageScratch,
    scratch_dirty: Vec<bool>,
    scratch_hashers: Vec<u64>,
    scratch_covered: Vec<bool>,
    scratch_occluded: Vec<bool>,
    scratch_under_clear: Vec<bool>,
    last_frame_damage: Option<Rect>,
    has_frame: bool,
}

impl CpuRasterizer {
    /// Creates a rasterizer for a `width` by `height` frame.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            frame_buffer: vec![0u32; (width * height) as usize],
            width,
            height,
            logical_width: width,
            logical_height: height,
            scale_factor: 1.0,
            display_list: DisplayList::new(),
            pending_damage: None,
            prev_items: Vec::new(),
            tile_hashes: Vec::new(),
            scratch_path: PathScratch::default(),
            scratch_row: ImageScratch::default(),
            scratch_dirty: Vec::new(),
            scratch_hashers: Vec::new(),
            scratch_covered: Vec::new(),
            scratch_occluded: Vec::new(),
            scratch_under_clear: Vec::new(),
            last_frame_damage: None,
            has_frame: false,
        }
    }

    /// The last frame's pixels as [`FrameOutput`] describes them: `0xAARRGGBB` words, sRGB,
    /// straight alpha, rows top to bottom.
    pub fn pixels(&self) -> &[u32] {
        &self.frame_buffer
    }

    /// The frame buffer as a raw pointer, its length in bytes, and its size.
    pub fn get_buffer(&self) -> (*const u8, usize, u32, u32) {
        (
            self.frame_buffer.as_ptr() as *const u8,
            self.frame_buffer.len() * 4,
            self.width,
            self.height,
        )
    }

    /// The current display list.
    pub fn display_list(&self) -> &DisplayList {
        &self.display_list
    }

    /// The size it draws at, in logical pixels.
    pub fn logical_size(&self) -> (u32, u32) {
        (self.logical_width, self.logical_height)
    }

    /// The buffer's size, in device pixels.
    pub fn pixel_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Physical pixels per logical one.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub(crate) fn path_coverage(
        path: &Path,
        paint: &Paint,
        origin: Point,
        (width, height): (u32, u32),
    ) -> AureaResult<Arc<[u8]>> {
        let shift = |p: Point| Point::new(p.x - origin.x, p.y - origin.y);
        let mut moved = Path::new();
        moved.fill_rule = path.fill_rule;
        moved.commands.reserve(path.commands.len());
        for command in &path.commands {
            moved.commands.push(match *command {
                PathCommand::MoveTo(p) => PathCommand::MoveTo(shift(p)),
                PathCommand::LineTo(p) => PathCommand::LineTo(shift(p)),
                PathCommand::QuadTo(c, p) => PathCommand::QuadTo(shift(c), shift(p)),
                PathCommand::CubicTo(a, b, p) => PathCommand::CubicTo(shift(a), shift(b), shift(p)),
                PathCommand::Close => PathCommand::Close,
            });
        }
        let white = Paint {
            color: Color::rgb(255, 255, 255),
            ..paint.clone()
        };
        let mut buf = vec![0u32; width as usize * height as usize];
        let mut scratch = PathScratch::default();
        Self::draw_path(
            &moved,
            &white,
            BlendMode::Normal,
            &mut buf,
            &mut scratch,
            width,
            ClipBox::surface(width, height),
        )?;
        for px in &mut buf {
            let a = px.to_be_bytes()[0];
            *px = u32::from_ne_bytes([a; 4]);
        }
        let bytes =
            unsafe { from_raw_parts(buf.as_ptr().cast::<u8>(), size_of_val(buf.as_slice())) };
        Ok(Arc::from(bytes))
    }

    pub(crate) fn render_recorded(&mut self, list: &mut DisplayList) -> AureaResult<FrameOutcome> {
        mem::swap(&mut self.display_list, list);
        let outcome = self.end_frame();
        mem::swap(&mut self.display_list, list);
        outcome
    }

    /// Padding, in pixels, on each outer-chord span edge so rounding never drops a pixel.
    const STROKE_SPAN_PAD: i32 = 2;

    #[inline]
    fn buf_set(
        buf: &mut [u32],
        w: u32,
        clip: ClipBox,
        x: i32,
        y: i32,
        color: u32,
        mode: BlendMode,
    ) {
        if !clip.contains(x, y) {
            return;
        }
        let idx = y.unsigned_abs() * w + x.unsigned_abs();
        if idx as usize >= buf.len() {
            return;
        }
        if mode == BlendMode::Normal && (color >> 24) == 255 {
            buf[idx as usize] = color;
        } else {
            let dst = buf[idx as usize];
            buf[idx as usize] = blend_pixel(color, dst, mode);
        }
    }

    /// Renders `item`, then keeps only what falls inside its clip paths, antialiased.
    fn render_clipped_item(
        item: &DisplayItem,
        buf: &mut [u32],
        scratch_path: &mut PathScratch,
        scratch_row: &mut ImageScratch,
        bw: u32,
        clip: ClipBox,
    ) -> AureaResult<()> {
        let Some(paths) = item.clip_paths.as_deref() else {
            return Self::render_item(item, buf, scratch_path, scratch_row, bw, clip);
        };
        if clip.is_empty() {
            return Ok(());
        }
        let (x0, x1) = (clip.x0 as usize, clip.x1 as usize);
        let (w, h) = (clip.x1 - clip.x0, clip.y1 - clip.y0);
        let mut saved = Vec::with_capacity(w as usize * h as usize);
        for y in clip.y0..clip.y1 {
            let row = y as usize * bw as usize;
            saved.extend_from_slice(&buf[row + x0..row + x1]);
        }
        Self::render_item(item, buf, scratch_path, scratch_row, bw, clip)?;

        let origin = Point::new(clip.x0 as f32, clip.y0 as f32);
        let mut coverage = vec![255u8; saved.len()];
        for path in paths {
            let mask = Self::path_coverage(path, &Paint::new(), origin, (w, h))?;
            for (c, m) in coverage.iter_mut().zip(mask.as_chunks::<4>().0) {
                *c = f32_to_u8_clamped((f32::from(*c) * f32::from(m[0]) / 255.0).round());
            }
        }
        let mut at = 0;
        for y in clip.y0..clip.y1 {
            let row = y as usize * bw as usize;
            for px in &mut buf[row + x0..row + x1] {
                let (old, cov) = (saved[at], u32::from(coverage[at]));
                at += 1;
                *px = match cov {
                    255 => *px,
                    0 => old,
                    _ => {
                        let mix = |shift: u32| {
                            let (o, n) = ((old >> shift) & 0xFF, (*px >> shift) & 0xFF);
                            ((o * (255 - cov) + n * cov + 127) / 255) << shift
                        };
                        mix(24) | mix(16) | mix(8) | mix(0)
                    }
                };
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_item(
        item: &DisplayItem,
        buf: &mut [u32],
        scratch_path: &mut PathScratch,
        scratch_row: &mut ImageScratch,
        bw: u32,
        clip: ClipBox,
    ) -> AureaResult<()> {
        if clip.is_empty() {
            return Ok(());
        }
        let alpha = item.opacity.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return Ok(());
        }
        match &item.command {
            DrawCommand::DrawRect(rect, paint) => {
                Self::draw_rect(
                    rect,
                    &fade_paint(paint, alpha),
                    item.blend_mode,
                    buf,
                    bw,
                    clip,
                );
            }
            DrawCommand::DrawRoundRect(rect, radius, paint) => {
                super::raster::round_rect::draw(
                    *rect,
                    *radius,
                    &fade_paint(paint, alpha),
                    item.blend_mode,
                    buf,
                    bw,
                    clip,
                );
            }
            DrawCommand::DrawShadow(rect, radius, blur, color) => {
                super::raster::round_rect::shadow(
                    *rect,
                    *radius,
                    *blur,
                    fade_color(*color, alpha),
                    item.blend_mode,
                    buf,
                    bw,
                    clip,
                );
            }
            DrawCommand::DrawCircle(center, radius, paint) => {
                Self::draw_circle(
                    *center,
                    *radius,
                    &fade_paint(paint, alpha),
                    item.blend_mode,
                    buf,
                    bw,
                    clip,
                );
            }
            DrawCommand::DrawPath(path, paint) => {
                Self::draw_path(
                    path,
                    &fade_paint(paint, alpha),
                    item.blend_mode,
                    buf,
                    scratch_path,
                    bw,
                    clip,
                )?;
            }
            DrawCommand::DrawGlyphMask(mask, origin, color) => {
                Self::draw_glyph_blended(
                    mask,
                    *origin,
                    fade_color(*color, alpha),
                    item.blend_mode,
                    buf,
                    bw,
                    clip,
                );
            }
            DrawCommand::DrawImageQuad(image, src, quad, sampling) => {
                Self::draw_image_quad(
                    image,
                    *src,
                    *quad,
                    *sampling,
                    item.blend_mode,
                    alpha,
                    buf,
                    bw,
                    clip,
                );
            }
            DrawCommand::DrawImageRect(image, dest, sampling) => {
                let src = Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
                Self::draw_image(
                    image,
                    src,
                    *dest,
                    *sampling,
                    item.blend_mode,
                    alpha,
                    buf,
                    scratch_row,
                    bw,
                    clip,
                );
            }
            DrawCommand::DrawImageRegion(image, src, dest, sampling) => {
                Self::draw_image(
                    image,
                    *src,
                    *dest,
                    *sampling,
                    item.blend_mode,
                    alpha,
                    buf,
                    scratch_row,
                    bw,
                    clip,
                );
            }
            DrawCommand::FillLinearGradient(grad, rect) => {
                let faded;
                let grad = if alpha < 1.0 {
                    faded = LinearGradient {
                        stops: fade_stops(&grad.stops, alpha),
                        ..grad.clone()
                    };
                    &faded
                } else {
                    grad
                };
                Self::fill_linear_gradient(grad, *rect, item.blend_mode, buf, bw, clip);
            }
            DrawCommand::FillRadialGradient(grad, rect) => {
                let faded;
                let grad = if alpha < 1.0 {
                    faded = RadialGradient {
                        stops: fade_stops(&grad.stops, alpha),
                        ..grad.clone()
                    };
                    &faded
                } else {
                    grad
                };
                Self::fill_radial_gradient(grad, *rect, item.blend_mode, buf, bw, clip);
            }
            _ => {}
        }
        Ok(())
    }

    fn clear_rect(rect: &Rect, color: u32, buf: &mut [u32], bw: u32, clip: ClipBox) {
        let x0 = f32_to_u32_clamped(rect.x.floor().clamp(clip.left(), clip.right()));
        let y0 = f32_to_u32_clamped(rect.y.floor().clamp(clip.top(), clip.bottom()));
        let x1 = f32_to_u32_clamped(
            (rect.x + rect.width)
                .ceil()
                .clamp(clip.left(), clip.right()),
        );
        let y1 = f32_to_u32_clamped(
            (rect.y + rect.height)
                .ceil()
                .clamp(clip.top(), clip.bottom()),
        );

        for y in y0..y1 {
            let start = (y * bw + x0) as usize;
            let end = (y * bw + x1) as usize;
            buf[start..end].fill(color);
        }
    }

    fn draw_rect(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let reach = match paint.style {
            PaintStyle::Fill => 0.0,
            PaintStyle::Stroke => paint.stroke_width / 2.0,
        };
        let x0 = f32_to_u32_clamped((rect.x - reach).floor().max(clip.left())).min(clip.x1);
        let y0 = f32_to_u32_clamped((rect.y - reach).floor().max(clip.top())).min(clip.y1);
        let x1 =
            f32_to_u32_clamped((rect.x + rect.width + reach).ceil().max(clip.left())).min(clip.x1);
        let y1 =
            f32_to_u32_clamped((rect.y + rect.height + reach).ceil().max(clip.top())).min(clip.y1);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        match paint.style {
            PaintStyle::Fill => {
                Self::fill_rect_region(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1)
            }
            PaintStyle::Stroke => {
                Self::stroke_rect_region(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_rect_region(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        if paint.color.a == 255 && mode == BlendMode::Normal && on_pixel_grid(rect) {
            let c = color_to_u32(paint.color);
            for y in y0..y1 {
                let start = (y * bw + x0) as usize;
                let end = (y * bw + x1) as usize;
                if end <= buf.len() {
                    buf[start..end].fill(c);
                }
            }
        } else {
            Self::fill_rect_region_translucent(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_rect_region_translucent(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        let xl = rect.x;
        let xr = rect.x + rect.width;
        let yl = rect.y;
        let yr = rect.y + rect.height;

        let xi0 = f32_to_i32_clamped(xl.ceil())
            .clamp(x0 as i32, x1 as i32)
            .unsigned_abs();
        let xi1 = f32_to_i32_clamped(xr.floor())
            .clamp(x0 as i32, x1 as i32)
            .unsigned_abs();
        let has_full_x = xi0 < xi1;
        let yi0 = f32_to_i32_clamped(yl.ceil())
            .clamp(y0 as i32, y1 as i32)
            .unsigned_abs();
        let yi1 = f32_to_i32_clamped(yr.floor())
            .clamp(y0 as i32, y1 as i32)
            .unsigned_abs();

        let const_src =
            (mode == BlendMode::Normal).then(|| ConstSrc::new(color_to_u32(paint.color)));
        let ctx = RectFillCtx {
            clip,
            paint,
            mode,
            xl,
            xr,
            c_full: color_to_u32(paint.color),
            opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
            const_src,
        };

        for y in y0..y1 {
            let row_cov = if y >= yi0 && y < yi1 {
                1.0
            } else {
                rect_cov_y(y, yl, yr)
            };
            if row_cov <= 0.0 {
                continue;
            }
            Self::fill_rect_row_translucent(
                buf, bw, &ctx, x0, x1, xi0, xi1, has_full_x, y, row_cov,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_rect_row_translucent(
        buf: &mut [u32],
        bw: u32,
        ctx: &RectFillCtx,
        x0: u32,
        x1: u32,
        xi0: u32,
        xi1: u32,
        has_full_x: bool,
        y: u32,
        row_cov: f32,
    ) {
        if !has_full_x {
            Self::fill_rect_edge_span(buf, bw, ctx, x0, x1, y, row_cov);
            return;
        }

        Self::fill_rect_edge_span(buf, bw, ctx, x0, xi0, y, row_cov);

        if row_cov >= 1.0 {
            if ctx.opaque_fast {
                let row_start = (y * bw) as usize;
                buf[row_start + xi0 as usize..row_start + xi1 as usize].fill(ctx.c_full);
            } else if let Some(src) = ctx.const_src {
                let row_start = (y * bw) as usize;
                let span = row_start + xi0 as usize..row_start + xi1 as usize;
                if let Some(pixels) = buf.get_mut(span) {
                    src.over_span(pixels);
                }
            } else {
                for x in xi0..xi1 {
                    Self::buf_set(buf, bw, ctx.clip, x as i32, y as i32, ctx.c_full, ctx.mode);
                }
            }
        } else {
            Self::blend_span_at(buf, bw, ctx, xi0, xi1, y, row_cov);
        }

        Self::fill_rect_edge_span(buf, bw, ctx, xi1, x1, y, row_cov);
    }

    /// [`Self::blend_at`] over `xa..xb` on row `y`, where every pixel has coverage `cov`.
    fn blend_span_at(
        buf: &mut [u32],
        bw: u32,
        ctx: &RectFillCtx,
        xa: u32,
        xb: u32,
        y: u32,
        cov: f32,
    ) {
        let inside = xa < xb
            && ctx.clip.contains(xa as i32, y as i32)
            && ctx.clip.contains((xb - 1) as i32, y as i32);
        if let (Some(src), true) = (ctx.const_src, inside) {
            let alpha = f32_to_u32_clamped((f32::from(ctx.paint.color.a) * cov).round());
            let row_start = (y * bw) as usize;
            if let Some(pixels) = buf.get_mut(row_start + xa as usize..row_start + xb as usize) {
                src.over_span_at_alpha(pixels, alpha);
                return;
            }
        }
        for x in xa..xb {
            Self::blend_at(buf, bw, ctx, x, y, cov);
        }
    }

    fn blend_at(buf: &mut [u32], bw: u32, ctx: &RectFillCtx, x: u32, y: u32, cov: f32) {
        let alpha = f32_to_u32_clamped((f32::from(ctx.paint.color.a) * cov).round());
        match ctx.const_src {
            Some(src) => {
                if !ctx.clip.contains(x as i32, y as i32) {
                    return;
                }
                let idx = (y * bw + x) as usize;
                if idx >= buf.len() {
                    return;
                }
                buf[idx] = src.over_at_alpha(buf[idx], alpha);
            }
            None => {
                let c = color_to_u32_with_coverage(ctx.paint.color, cov);
                Self::buf_set(buf, bw, ctx.clip, x as i32, y as i32, c, ctx.mode);
            }
        }
    }

    fn fill_rect_edge_span(
        buf: &mut [u32],
        bw: u32,
        ctx: &RectFillCtx,
        xa: u32,
        xb: u32,
        y: u32,
        row_cov: f32,
    ) {
        for x in xa..xb {
            let cov = rect_cov_x(x, ctx.xl, ctx.xr) * row_cov;
            if cov <= 0.0 {
                continue;
            }
            Self::blend_at(buf, bw, ctx, x, y, cov);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stroke_rect_region(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        let half = paint.stroke_width / 2.0;
        if half <= 0.0 {
            return;
        }
        let outer = Rect::new(
            rect.x - half,
            rect.y - half,
            rect.width + 2.0 * half,
            rect.height + 2.0 * half,
        );
        let inner = Rect::new(
            rect.x + half,
            rect.y + half,
            (rect.width - 2.0 * half).max(0.0),
            (rect.height - 2.0 * half).max(0.0),
        );
        if inner.width <= 0.0 || inner.height <= 0.0 {
            let fill = paint.clone().style(PaintStyle::Fill);
            return Self::fill_rect_region(&outer, &fill, mode, buf, bw, clip, x0, y0, x1, y1);
        }
        let const_src =
            (mode == BlendMode::Normal).then(|| ConstSrc::new(color_to_u32(paint.color)));
        let ctx = RectFillCtx {
            clip,
            paint,
            mode,
            xl: outer.x,
            xr: outer.right(),
            c_full: color_to_u32(paint.color),
            opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
            const_src,
        };
        let band = |from: f32, to: f32| {
            f32_to_u32_clamped(from.floor()).clamp(x0, x1)
                ..f32_to_u32_clamped(to.ceil()).clamp(x0, x1)
        };
        let left = band(outer.x, inner.x);
        let right = band(inner.right(), outer.right());
        for y in y0..y1 {
            let out_y = rect_cov_y(y, outer.y, outer.bottom());
            if out_y <= 0.0 {
                continue;
            }
            let in_y = rect_cov_y(y, inner.y, inner.bottom());
            let cov_at = |x: u32| {
                out_y * rect_cov_x(x, outer.x, outer.right())
                    - in_y * rect_cov_x(x, inner.x, inner.right())
            };
            let stroke_at = |buf: &mut [u32], x: u32| {
                let cov = cov_at(x);
                if cov <= 0.0 {
                    return;
                }
                if cov >= 1.0 && ctx.opaque_fast {
                    Self::buf_set(buf, bw, clip, x as i32, y as i32, ctx.c_full, mode);
                } else {
                    Self::blend_at(buf, bw, &ctx, x, y, cov.min(1.0));
                }
            };
            if in_y >= 1.0 {
                for x in left.clone().chain(right.clone()) {
                    stroke_at(buf, x);
                }
                continue;
            }
            // Only the columns holding an outer or inner edge can have a coverage that differs
            // from their neighbours, so each run between them is one span.
            let mut edges = [u32::MAX; 4];
            for (slot, e) in edges
                .iter_mut()
                .zip([outer.x, outer.right(), inner.x, inner.right()])
            {
                let f = e.floor();
                if f >= x0 as f32 && f < x1 as f32 {
                    *slot = f32_to_u32_clamped(f);
                }
            }
            edges.sort_unstable();
            let run = |buf: &mut [u32], from: u32, to: u32| {
                if from < to {
                    let cov = cov_at(from);
                    if cov > 0.0 {
                        Self::blend_span_at(buf, bw, &ctx, from, to, y, cov.min(1.0));
                    }
                }
            };
            let mut from = x0;
            for e in edges {
                if e >= x1 || e < from {
                    continue;
                }
                run(buf, from, e);
                stroke_at(buf, e);
                from = e + 1;
            }
            run(buf, from, x1);
        }
    }

    fn draw_circle(
        center: Point,
        radius: f32,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let x0 = f32_to_u32_clamped((center.x - radius).floor().max(clip.left())).min(clip.x1);
        let y0 = f32_to_u32_clamped((center.y - radius).floor().max(clip.top())).min(clip.y1);
        let x1 = f32_to_u32_clamped((center.x + radius).ceil().max(clip.left())).min(clip.x1);
        let y1 = f32_to_u32_clamped((center.y + radius).ceil().max(clip.top())).min(clip.y1);

        match paint.style {
            PaintStyle::Fill => {
                Self::fill_circle_region(
                    center, radius, paint, mode, buf, bw, clip, x0, y0, x1, y1,
                );
            }
            PaintStyle::Stroke => {
                Self::stroke_circle_region(
                    center, radius, paint, mode, buf, bw, clip, x0, y0, x1, y1,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_circle_region(
        center: Point,
        radius: f32,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        let ctx = CircleFillCtx {
            clip,
            center,
            radius,
            paint,
            mode,
            r_in: (radius - 0.5).max(0.0),
            r_out: radius + 0.5,
            c_full: color_to_u32(paint.color),
            opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
            const_src: (mode == BlendMode::Normal)
                .then(|| ConstSrc::new(color_to_u32(paint.color))),
        };

        for y in y0..y1 {
            Self::fill_circle_row(buf, bw, &ctx, x0, x1, y);
        }
    }

    fn fill_circle_row(buf: &mut [u32], bw: u32, ctx: &CircleFillCtx, x0: u32, x1: u32, y: u32) {
        let dy = y as f32 + 0.5 - ctx.center.y;
        if dy.abs() >= ctx.r_out {
            return;
        }
        let half_out = (ctx.r_out * ctx.r_out - dy * dy).max(0.0).sqrt();
        let xo0 =
            f32_to_i32_clamped((ctx.center.x - half_out).floor().max(x0 as f32)).min(x1 as i32);
        let xo1 =
            f32_to_i32_clamped((ctx.center.x + half_out).ceil().min(x1 as f32)).max(x0 as i32);

        let (xi0, xi1) = if dy.abs() < ctx.r_in {
            let half_in = (ctx.r_in * ctx.r_in - dy * dy).max(0.0).sqrt();
            let a = f32_to_i32_clamped((ctx.center.x - half_in).ceil());
            let b = f32_to_i32_clamped((ctx.center.x + half_in).floor());
            if a < b {
                (a.clamp(xo0, xo1), b.clamp(xo0, xo1))
            } else {
                (xo0, xo0)
            }
        } else {
            (xo0, xo0)
        };

        Self::fill_circle_edge_span(buf, bw, ctx, xo0, xi0, y);

        if xi0 < xi1 {
            let row_start = (y * bw) as usize;
            let span =
                row_start + xi0.unsigned_abs() as usize..row_start + xi1.unsigned_abs() as usize;
            if ctx.opaque_fast {
                buf[span].fill(ctx.c_full);
            } else if let (Some(src), Some(pixels)) = (ctx.const_src, buf.get_mut(span)) {
                src.over_span(pixels);
            } else {
                for x in xi0..xi1 {
                    Self::buf_set(buf, bw, ctx.clip, x, y as i32, ctx.c_full, ctx.mode);
                }
            }
        }

        Self::fill_circle_edge_span(buf, bw, ctx, xi1, xo1, y);
    }

    fn fill_circle_edge_span(
        buf: &mut [u32],
        bw: u32,
        ctx: &CircleFillCtx,
        xa: i32,
        xb: i32,
        y: u32,
    ) {
        for x in xa..xb {
            let cov = circle_coverage(ctx.center, ctx.radius, x as f32, y as f32);
            if cov <= 0.0 {
                continue;
            }
            match ctx.const_src {
                Some(src) if ctx.clip.contains(x, y as i32) => {
                    let alpha = f32_to_u32_clamped((f32::from(ctx.paint.color.a) * cov).round());
                    if let Some(p) = buf.get_mut((y * bw) as usize + x.unsigned_abs() as usize) {
                        *p = src.over_at_alpha(*p, alpha);
                    }
                }
                Some(_) => {}
                None => {
                    let c = color_to_u32_with_coverage(ctx.paint.color, cov);
                    Self::buf_set(buf, bw, ctx.clip, x, y as i32, c, ctx.mode);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stroke_circle_region(
        center: Point,
        radius: f32,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        let ctx = CircleStrokeCtx {
            clip,
            center,
            radius,
            inner_r: (radius - paint.stroke_width).max(0.0),
            c: color_to_u32(paint.color),
            mode,
        };
        for y in y0..y1 {
            Self::stroke_circle_row(buf, bw, &ctx, x0, x1, y);
        }
    }

    /// Splits a row into the spans that can hold stroke pixels: the outer chord, minus the
    /// inner chord when it is wide enough to skip. Pixels outside those spans fail the
    /// per-pixel test in `stroke_circle_span` by a margin, so they are never tested.
    fn stroke_circle_row(
        buf: &mut [u32],
        bw: u32,
        ctx: &CircleStrokeCtx,
        x0: u32,
        x1: u32,
        y: u32,
    ) {
        let dy = y as f32 + 0.5 - ctx.center.y;
        let half_out = (ctx.radius * ctx.radius - dy * dy).max(0.0).sqrt();
        let pad = Self::STROKE_SPAN_PAD;
        let lo = (f32_to_i32_clamped((ctx.center.x - half_out).floor()) - pad).max(x0 as i32);
        let hi = (f32_to_i32_clamped((ctx.center.x + half_out).ceil()) + pad).min(x1 as i32);
        if lo >= hi {
            return;
        }
        let half_in = (ctx.inner_r * ctx.inner_r - dy * dy).max(0.0).sqrt();
        if half_in < 1.0 {
            Self::stroke_circle_span(buf, bw, ctx, y, lo, hi);
            return;
        }
        // Centres in [skip_lo, skip_hi) lie at least a pixel inside the hole.
        let skip_lo = f32_to_i32_clamped((ctx.center.x - half_in + 0.5).ceil());
        let skip_hi = f32_to_i32_clamped((ctx.center.x + half_in - 1.5).floor()) + 1;
        Self::stroke_circle_span(buf, bw, ctx, y, lo, skip_lo.min(hi));
        Self::stroke_circle_span(buf, bw, ctx, y, skip_hi.max(lo), hi);
    }

    fn stroke_circle_span(
        buf: &mut [u32],
        bw: u32,
        ctx: &CircleStrokeCtx,
        y: u32,
        xa: i32,
        xb: i32,
    ) {
        let dy = y as f32 + 0.5 - ctx.center.y;
        for x in xa..xb {
            let dx = x as f32 + 0.5 - ctx.center.x;
            let d = (dx * dx + dy * dy).sqrt();
            if d <= ctx.radius && d >= ctx.inner_r {
                Self::buf_set(buf, bw, ctx.clip, x, y as i32, ctx.c, ctx.mode);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_path(
        path: &Path,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        scratch_path: &mut PathScratch,
        bw: u32,
        clip: ClipBox,
    ) -> AureaResult<()> {
        if paint.style == PaintStyle::Stroke {
            let outline = stroke::outline_path(path, &stroke::StrokeStyle::of(paint));
            return Self::fill_path(&outline, paint, mode, buf, scratch_path, bw, clip);
        }

        Self::fill_path(path, paint, mode, buf, scratch_path, bw, clip)
    }

    fn fill_path(
        path: &Path,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        scratch_path: &mut PathScratch,
        bw: u32,
        clip: ClipBox,
    ) -> AureaResult<()> {
        fill::fill_path(path, paint.color, mode, buf, bw, clip, scratch_path);
        Ok(())
    }

    #[cfg(test)]
    fn draw_glyph(
        mask: &GlyphMask,
        origin: Point,
        color: Color,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        Self::draw_glyph_blended(mask, origin, color, BlendMode::Normal, buf, bw, clip);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_glyph_blended(
        mask: &GlyphMask,
        origin: Point,
        color: Color,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        if mask.width == 0
            || mask.height == 0
            || color.a == 0
            || !bytes_fit(mask.width, mask.height, 3, mask.coverage.len())
        {
            return;
        }
        let luts = LinearLuts::get();
        let (tr, tg, tb) = (
            luts.linear(color.r),
            luts.linear(color.g),
            luts.linear(color.b),
        );
        let opaque_pixel = 0xFF00_0000
            | (u32::from(color.r) << 16)
            | (u32::from(color.g) << 8)
            | u32::from(color.b);
        let source = ConstSrc::new(opaque_pixel);
        let dx = f32_to_i32_clamped(origin.x.round());
        let dy = f32_to_i32_clamped(origin.y.round());

        let offset = |edge: u32, origin: i32, limit: u32| {
            usize::try_from((i64::from(edge) - i64::from(origin)).clamp(0, i64::from(limit)))
                .unwrap_or(0)
        };
        let x_lo = offset(clip.x0, dx, mask.width);
        let x_hi = offset(clip.x1, dx, mask.width);
        let y_lo = offset(clip.y0, dy, mask.height);
        let y_hi = offset(clip.y1, dy, mask.height);
        if x_lo >= x_hi || y_lo >= y_hi {
            return;
        }

        let px = usize::try_from(i64::from(dx) + x_lo as i64).unwrap_or(0);
        let py = usize::try_from(i64::from(dy) + y_lo as i64).unwrap_or(0);
        let alpha = u16::from(color.a);
        // Colour alpha applied to each coverage byte; the identity at 255.
        let mut fade = [0u8; 256];
        for (c, slot) in fade.iter_mut().enumerate() {
            let c = u16::try_from(c).unwrap_or(0);
            *slot = u8::try_from((c * alpha + 127) / 255).unwrap_or(255);
        }
        let normal = mode == BlendMode::Normal;
        let rgb = opaque_pixel & 0x00ff_ffff;
        for my in y_lo..y_hi {
            let row = my * mask.width as usize;
            let cov_row = &mask.coverage[(row + x_lo) * 3..(row + x_hi) * 3];
            let start = (py + my - y_lo) * bw as usize + px;
            let dst_row = &mut buf[start..start + x_hi - x_lo];

            for (coverage, dst) in cov_row.as_chunks::<3>().0.iter().zip(dst_row) {
                let cr8 = fade[usize::from(coverage[0])];
                let cg8 = fade[usize::from(coverage[1])];
                let cb8 = fade[usize::from(coverage[2])];
                if cr8 == 0 && cg8 == 0 && cb8 == 0 {
                    continue;
                }

                if normal && cr8 == 255 && cg8 == 255 && cb8 == 255 {
                    *dst = opaque_pixel;
                    continue;
                }
                if !normal || (*dst >> 24) != 255 {
                    let coverage = u32::from(cr8.max(cg8).max(cb8));
                    *dst = if normal {
                        source.over_at_alpha(*dst, coverage)
                    } else {
                        blend_pixel((coverage << 24) | rgb, *dst, mode)
                    };
                    continue;
                }
                let da = (*dst >> 24) & 0xff;
                let [_, dr, dg, db] = dst.to_be_bytes();
                let or_ = luts.mix(tr, dr, cr8);
                let og = luts.mix(tg, dg, cg8);
                let ob = luts.mix(tb, db, cb8);
                let sa = u32::from(cr8.max(cg8).max(cb8));
                let oa = sa + ((255 - sa) * da) / 255;
                *dst = (oa << 24) | (or_ << 16) | (og << 8) | ob;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_image_quad(
        image: &Image,
        src: Rect,
        q: [Point; 3],
        sampling: Sampling,
        mode: BlendMode,
        alpha: f32,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        if image.width == 0
            || image.height == 0
            || !bytes_fit(image.width, image.height, 4, image.data.len())
            || ![src.x, src.y, src.width, src.height]
                .iter()
                .all(|v| v.is_finite())
            || src.width <= 0.0
            || src.height <= 0.0
            || !q.iter().all(|p| p.x.is_finite() && p.y.is_finite())
        {
            return;
        }
        let ax = q[1].x - q[0].x;
        let ay = q[1].y - q[0].y;
        let bx = q[2].x - q[0].x;
        let by = q[2].y - q[0].y;
        let det = ax * by - ay * bx;
        if !det.is_finite() || det.abs() < 1e-12 {
            return;
        }
        let inverse = det.recip();
        let b = quad_bounds(&q);
        let x0 = f32_to_i32_clamped(b.x.floor().max(clip.left()));
        let y0 = f32_to_i32_clamped(b.y.floor().max(clip.top()));
        let x1 = f32_to_i32_clamped(b.right().ceil().min(clip.right()));
        let y1 = f32_to_i32_clamped(b.bottom().ceil().min(clip.bottom()));
        let iw = i32::try_from(image.width).unwrap_or(i32::MAX);
        let ih = i32::try_from(image.height).unwrap_or(i32::MAX);
        let bounds = sample::Bounds {
            x0: f32_to_i32_clamped(src.x.floor()).clamp(0, iw - 1),
            y0: f32_to_i32_clamped(src.y.floor()).clamp(0, ih - 1),
            x1: (f32_to_i32_clamped(src.right().ceil()).saturating_sub(1)).clamp(0, iw - 1),
            y1: (f32_to_i32_clamped(src.bottom().ceil()).saturating_sub(1)).clamp(0, ih - 1),
        };
        let footprint_x = ((by.abs() + bx.abs()) / det.abs() * src.width).max(1.0);
        let footprint_y = ((ay.abs() + ax.abs()) / det.abs() * src.height).max(1.0);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - q[0].x;
                let dy = y as f32 + 0.5 - q[0].y;
                let u = (dx * by - dy * bx) * inverse;
                let v = (dy * ax - dx * ay) * inverse;
                if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                    continue;
                }
                let u = src.x + u * src.width;
                let v = src.y + v * src.height;
                let c = match sampling {
                    Sampling::Nearest => sample::nearest(image, bounds, u, v, alpha),
                    Sampling::Smooth if footprint_x > 1.0 || footprint_y > 1.0 => sample::area(
                        image,
                        bounds,
                        (u - footprint_x * 0.5, v - footprint_y * 0.5),
                        (u + footprint_x * 0.5, v + footprint_y * 0.5),
                        alpha,
                    ),
                    _ => sample::bilinear(image, bounds, u, v, alpha),
                };
                if c >> 24 != 0 {
                    Self::buf_set(buf, bw, clip, x, y, c, mode);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_image(
        image: &Image,
        src: Rect,
        dest: Rect,
        sampling: Sampling,
        mode: BlendMode,
        alpha: f32,
        buf: &mut [u32],
        scratch: &mut ImageScratch,
        bw: u32,
        clip: ClipBox,
    ) {
        if image.width == 0
            || image.height == 0
            || !bytes_fit(image.width, image.height, 4, image.data.len())
            || !(src.width > 0.0 && src.height > 0.0)
            || dest.width <= 0.0
            || dest.height <= 0.0
        {
            return;
        }
        let centre_span = |from: f32, to: f32, lo: u32, hi: u32| {
            (
                f32_to_i32_clamped((from - 0.5).ceil()).max(lo as i32),
                f32_to_i32_clamped((to - 0.5).ceil()).min(hi as i32),
            )
        };
        let (x0, x1) = centre_span(dest.x, dest.x + dest.width, clip.x0, clip.x1);
        let (y0, y1) = centre_span(dest.y, dest.y + dest.height, clip.y0, clip.y1);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        if (src.width - dest.width).abs() < 0.001 && (src.height - dest.height).abs() < 0.001 {
            Self::draw_image_1to1(
                image,
                src,
                dest,
                mode,
                alpha,
                buf,
                &mut scratch.row,
                bw,
                clip,
                x0,
                y0,
                x1,
                y1,
            );
        } else if sampling == Sampling::Nearest {
            Self::draw_image_scaled(
                image, src, dest, mode, alpha, buf, bw, x0, y0, x1, y1, scratch,
            );
        } else {
            Self::draw_image_filtered(
                image,
                src,
                dest,
                sampling,
                mode,
                alpha,
                buf,
                scratch,
                bw,
                clip,
                (x0, y0, x1, y1),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_image_1to1(
        image: &Image,
        src: Rect,
        dest: Rect,
        mode: BlendMode,
        alpha: f32,
        buf: &mut [u32],
        scratch_row: &mut Vec<u32>,
        bw: u32,
        clip: ClipBox,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
    ) {
        let max_sx = image.width as f32 - 0.001;
        let max_sy = image.height as f32 - 0.001;
        let sx0 = (x0 as f32 - dest.x) + src.x;
        let row_buf = scratch_row;
        row_buf.clear();
        row_buf.resize(usize::try_from(x1 - x0).expect("x1 > x0"), 0u32);
        for cy in y0..y1 {
            let v = (cy as f32 - dest.y) + src.y;
            let sy = f32_to_u32_clamped(v.clamp(0.0, max_sy));
            let src_row = &image.data[sy as usize * image.width as usize * 4..];
            let mut all_opaque = true;
            for (i, slot) in row_buf.iter_mut().enumerate() {
                let sx = f32_to_usize_clamped((sx0 + i as f32).clamp(0.0, max_sx));
                let ii = sx * 4;
                if ii + 3 >= src_row.len() {
                    *slot = 0;
                    all_opaque = false;
                    continue;
                }
                let a = scale_u8(src_row[ii + 3], alpha);
                if a != 255 {
                    all_opaque = false;
                }
                *slot = (u32::from(a) << 24)
                    | (u32::from(src_row[ii]) << 16)
                    | (u32::from(src_row[ii + 1]) << 8)
                    | u32::from(src_row[ii + 2]);
            }
            if mode == BlendMode::Normal && all_opaque {
                let row_start = (cy.unsigned_abs() * bw + x0.unsigned_abs()) as usize;
                buf[row_start..row_start + row_buf.len()].copy_from_slice(row_buf.as_slice());
            } else {
                for (i, &c) in row_buf.iter().enumerate() {
                    let xi = i32::try_from(i).expect("row width fits in i32");
                    Self::buf_set(buf, bw, clip, x0 + xi, cy, c, mode);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_image_filtered(
        image: &Image,
        src: Rect,
        dest: Rect,
        sampling: Sampling,
        mode: BlendMode,
        alpha: f32,
        buf: &mut [u32],
        scratch: &mut ImageScratch,
        bw: u32,
        clip: ClipBox,
        (x0, y0, x1, y1): (i32, i32, i32, i32),
    ) {
        let (iw, ih) = (
            i32::try_from(image.width).unwrap_or(i32::MAX),
            i32::try_from(image.height).unwrap_or(i32::MAX),
        );
        if iw == 0 || ih == 0 {
            return;
        }
        let bounds = sample::Bounds {
            x0: f32_to_i32_clamped(src.x.floor()).clamp(0, iw - 1),
            y0: f32_to_i32_clamped(src.y.floor()).clamp(0, ih - 1),
            x1: (f32_to_i32_clamped((src.x + src.width).ceil()) - 1).clamp(0, iw - 1),
            y1: (f32_to_i32_clamped((src.y + src.height).ceil()) - 1).clamp(0, ih - 1),
        };
        let (sx, sy) = (src.width / dest.width, src.height / dest.height);
        let shrink = sampling == Sampling::Smooth && (sx > 1.0 || sy > 1.0);
        let u0 = |cx: i32| (cx as f32 - dest.x) * sx + src.x;
        let v0 = |cy: i32| (cy as f32 - dest.y) * sy + src.y;
        let put = |buf: &mut [u32], cx: i32, cy: i32, c: u32| {
            if c >> 24 != 0 {
                Self::buf_set(buf, bw, clip, cx, cy, c, mode);
            }
        };
        let ImageScratch {
            row: line,
            columns,
            offsets,
            rows,
            taps,
            ..
        } = scratch;
        if shrink {
            columns.clear();
            offsets.clear();
            for cx in x0..x1 {
                offsets.push(columns.len());
                let u = u0(cx);
                columns.extend(sample::area_weights(u, u + sx, bounds.x0, bounds.x1));
            }
            offsets.push(columns.len());
            for cy in y0..y1 {
                let v = v0(cy);
                rows.clear();
                rows.extend(sample::area_weights(v, v + sy, bounds.y0, bounds.y1));
                for (i, cx) in (x0..x1).enumerate() {
                    let xs = columns[offsets[i]..offsets[i + 1]].iter().copied();
                    put(
                        buf,
                        cx,
                        cy,
                        sample::area_from(image, xs, rows.iter().copied(), alpha),
                    );
                }
            }
        } else if bounds.x0 <= bounds.x1 && bounds.y0 <= bounds.y1 {
            taps.clear();
            taps.extend(
                (x0..x1).map(|cx| sample::Taps::at(u0(cx) + sx * 0.5, bounds.x0, bounds.x1)),
            );
            line.clear();
            line.resize(taps.len(), 0);
            for cy in y0..y1 {
                let row = sample::Taps::at(v0(cy) + sy * 0.5, bounds.y0, bounds.y1);
                if sample::bilinear_row(image, taps, row, alpha, line) && mode == BlendMode::Normal
                {
                    let start = (cy.unsigned_abs() * bw + x0.unsigned_abs()) as usize;
                    buf[start..start + line.len()].copy_from_slice(line);
                } else {
                    for (&c, cx) in line.iter().zip(x0..x1) {
                        put(buf, cx, cy, c);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_image_scaled(
        image: &Image,
        src: Rect,
        dest: Rect,
        mode: BlendMode,
        alpha: f32,
        buf: &mut [u32],
        bw: u32,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        scratch: &mut ImageScratch,
    ) {
        // The caller has checked that the pixel data fills `width * height`, and the clamps keep
        // every index inside it. The span is already inside the clip.
        let ImageScratch {
            row, source_cols, ..
        } = scratch;
        let max_sx = image.width as f32 - 0.001;
        let max_sy = image.height as f32 - 0.001;
        let stride = image.width as usize * 4;
        source_cols.clear();
        source_cols.extend((x0..x1).map(|cx| {
            let u = (cx as f32 + 0.5 - dest.x) / dest.width * src.width + src.x;
            f32_to_u32_clamped(u.clamp(0.0, max_sx)) as usize * 4
        }));
        row.clear();
        row.resize(source_cols.len(), 0);
        for cy in y0..y1 {
            let v = (cy as f32 + 0.5 - dest.y) / dest.height * src.height + src.y;
            let sy = f32_to_u32_clamped(v.clamp(0.0, max_sy)) as usize;
            let src_row = &image.data[sy * stride..(sy + 1) * stride];
            let mut all_opaque = true;
            for (slot, &ii) in row.iter_mut().zip(source_cols.iter()) {
                let a = scale_u8(src_row[ii + 3], alpha);
                all_opaque &= a == 255;
                *slot = (u32::from(a) << 24)
                    | (u32::from(src_row[ii]) << 16)
                    | (u32::from(src_row[ii + 1]) << 8)
                    | u32::from(src_row[ii + 2]);
            }
            let start = (cy.unsigned_abs() * bw + x0.unsigned_abs()) as usize;
            if mode == BlendMode::Normal && all_opaque {
                buf[start..start + row.len()].copy_from_slice(row.as_slice());
            } else {
                for (i, &c) in row.iter().enumerate() {
                    if c >> 24 != 0 {
                        let idx = start + i;
                        buf[idx] = blend_pixel(c, buf[idx], mode);
                    }
                }
            }
        }
    }

    fn gradient_color_at(stops: &[GradientStop], t: f32) -> u32 {
        let t = t.clamp(0.0, 1.0);
        if stops.is_empty() {
            return 0;
        }
        if stops.len() == 1 {
            let c = stops[0].color;
            return (u32::from(c.a) << 24)
                | (u32::from(c.r) << 16)
                | (u32::from(c.g) << 8)
                | u32::from(c.b);
        }
        for w in stops.windows(2) {
            let (a, b) = (w[0].offset, w[1].offset);
            if t >= a && t <= b {
                let s = if (b - a).abs() < 1e-6 {
                    1.0
                } else {
                    (t - a) / (b - a)
                };
                let (c0, c1) = (w[0].color, w[1].color);
                let lerp = |a: u8, b: u8| {
                    f32_to_u8_clamped((f32::from(a) + (f32::from(b) - f32::from(a)) * s).round())
                };
                let (r, g, b_, a_) = (
                    lerp(c0.r, c1.r),
                    lerp(c0.g, c1.g),
                    lerp(c0.b, c1.b),
                    lerp(c0.a, c1.a),
                );
                return (u32::from(a_) << 24)
                    | (u32::from(r) << 16)
                    | (u32::from(g) << 8)
                    | u32::from(b_);
            }
        }
        let c = if t <= stops[0].offset {
            stops[0].color
        } else {
            stops.last().expect("stops has at least 2 elements").color
        };
        (u32::from(c.a) << 24) | (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
    }

    fn build_gradient_lut(stops: &[GradientStop]) -> [u32; 256] {
        let mut lut = [0u32; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            *slot = Self::gradient_color_at(stops, i as f32 / 255.0);
        }
        lut
    }

    fn fill_linear_gradient(
        grad: &LinearGradient,
        rect: Rect,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let (start, end) = grad.pixel_axis();
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq < 1e-10 {
            return;
        }
        let lut = Self::build_gradient_lut(&grad.stops);
        let x0 = f32_to_i32_clamped(rect.x.max(clip.left()).ceil());
        let y0 = f32_to_i32_clamped(rect.y.max(clip.top()).ceil());
        let x1 = f32_to_i32_clamped((rect.x + rect.width).min(clip.right()).floor());
        let y1 = f32_to_i32_clamped((rect.y + rect.height).min(clip.bottom()).floor());
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let dt_x = dx / len_sq;
        let luts = LinearLuts::get();
        let steps = (mode == BlendMode::Normal).then(|| gradient_steps(&lut, luts));

        for cy in y0..y1 {
            let row = (cy.unsigned_abs() * bw) as usize;
            let mut t =
                ((x0 as f32 + 0.5 - start.x) * dx + (cy as f32 + 0.5 - start.y) * dy) / len_sq;
            for cx in x0..x1 {
                let t_idx = f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round());
                let idx = row + cx.unsigned_abs() as usize;
                buf[idx] = match &steps {
                    Some(steps) => gradient_over(&steps[t_idx], buf[idx], luts),
                    None => blend_pixel(lut[t_idx], buf[idx], mode),
                };
                t += dt_x;
            }
        }
    }

    /// Smallest squared distance from the centre that reaches each step, for `fill_radial_gradient`.
    /// Entry `j` is the threshold for step `j + 1`. Found by bisection with the per-pixel
    /// `sqrt`-and-round arithmetic, so a lookup gives the same step as computing it per pixel.
    fn radial_thresholds(inv_radius: f32) -> [f32; 255] {
        let step_of_sq = |d2: f32| {
            let t = (d2.sqrt() * inv_radius).min(1.0);
            f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round())
        };
        let mut thresholds = [f32::INFINITY; 255];
        let largest = step_of_sq(f32::MAX);
        for (j, slot) in thresholds.iter_mut().enumerate() {
            let target = j + 1;
            if largest < target {
                continue;
            }
            let (mut lo, mut hi) = (0u32, f32::MAX.to_bits());
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if step_of_sq(f32::from_bits(mid)) >= target {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            *slot = f32::from_bits(lo);
        }
        thresholds
    }

    fn fill_radial_gradient(
        grad: &RadialGradient,
        rect: Rect,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        if grad.radius <= 0.0 {
            return;
        }
        let lut = Self::build_gradient_lut(&grad.stops);
        let x0 = f32_to_i32_clamped(rect.x.max(clip.left()).ceil());
        let y0 = f32_to_i32_clamped(rect.y.max(clip.top()).ceil());
        let x1 = f32_to_i32_clamped((rect.x + rect.width).min(clip.right()).floor());
        let y1 = f32_to_i32_clamped((rect.y + rect.height).min(clip.bottom()).floor());
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let inv_radius = 1.0 / grad.radius;
        let thresholds = Self::radial_thresholds(inv_radius);
        let luts = LinearLuts::get();
        let steps = (mode == BlendMode::Normal).then(|| gradient_steps(&lut, luts));

        for cy in y0..y1 {
            let row = (cy.unsigned_abs() * bw) as usize;
            let dy = cy as f32 + 0.5 - grad.center.y;
            let dy_sq = dy * dy;
            // Each pixel's step is found by walking from the previous one. Squared distance is
            // monotone in the step, so the walk stops where the threshold table says it should.
            let dx0 = x0 as f32 + 0.5 - grad.center.x;
            let mut step = thresholds.partition_point(|&th| th <= dx0 * dx0 + dy_sq);
            for cx in x0..x1 {
                let dx = cx as f32 + 0.5 - grad.center.x;
                let d2 = dx * dx + dy_sq;
                while step < 255 && d2 >= thresholds[step] {
                    step += 1;
                }
                while step > 0 && d2 < thresholds[step - 1] {
                    step -= 1;
                }
                let idx = row + cx.unsigned_abs() as usize;
                buf[idx] = match &steps {
                    Some(steps) => gradient_over(&steps[step], buf[idx], luts),
                    None => blend_pixel(lut[step], buf[idx], mode),
                };
            }
        }
    }

    /// What changed since the last frame. Items the two lists share at the start and the end are
    /// matched by cache key, so drawing a different number of items in the middle (a tile pyramid
    /// changing level) damages only the middle, not everything after it.
    fn diff_damage(&self) -> FrameDamage {
        let new_items = self.display_list.items();
        let old_items = &self.prev_items;
        let same = |n: &DisplayItem, o: &(CacheKey, Rect)| n.cache_key == o.0;
        let prefix = new_items
            .iter()
            .zip(old_items)
            .take_while(|(n, o)| same(n, o))
            .count();
        let suffix = new_items[prefix..]
            .iter()
            .rev()
            .zip(old_items[prefix..].iter().rev())
            .take_while(|(n, o)| same(n, o))
            .count();
        let new_mid = &new_items[prefix..new_items.len() - suffix];
        let old_mid = &old_items[prefix..old_items.len() - suffix];

        let mut acc: Option<Rect> = None;
        let mut add = |r: Rect| -> bool {
            if !is_known_bounds(r) {
                return false;
            }
            acc = Some(acc.map_or(r, |a| union_rect(a, r)));
            true
        };
        if new_mid.len() == old_mid.len() {
            for (n, o) in new_mid.iter().zip(old_mid) {
                if !same(n, o) && !(add(n.bounds) && add(o.1)) {
                    return FrameDamage::Full;
                }
            }
        } else {
            let bounds = new_mid
                .iter()
                .map(|n| n.bounds)
                .chain(old_mid.iter().map(|o| o.1));
            for r in bounds {
                if !add(r) {
                    return FrameDamage::Full;
                }
            }
        }

        match acc {
            Some(r) => FrameDamage::Region(r),
            None => FrameDamage::Unchanged,
        }
    }

    fn compute_dirty_tiles(
        &mut self,
        damage: Option<Rect>,
        forced: Option<Rect>,
        tiles_x: u32,
        tiles_y: u32,
    ) -> Vec<bool> {
        let tile_count = (tiles_x * tiles_y) as usize;
        if self.tile_hashes.len() != tile_count {
            self.tile_hashes = vec![0u64; tile_count];
        }

        let mut dirty = mem::take(&mut self.scratch_dirty);
        dirty.clear();
        dirty.resize(tile_count, false);

        let range = match damage {
            Some(rect) => tile_range(rect, tiles_x, tiles_y),
            None => (0, 0, tiles_x, tiles_y),
        };
        self.refine_tile_hashes(range, (tiles_x, tiles_y), &mut dirty);

        if damage.is_none() {
            dirty.fill(true);
        }
        if let Some(rect) = forced {
            mark_tile_range_dirty(rect, tiles_x, tiles_y, &mut dirty);
        }
        dirty
    }

    fn refine_tile_hashes(
        &mut self,
        (tx0, ty0, tx1, ty1): (u32, u32, u32, u32),
        (tiles_x, tiles_y): (u32, u32),
        dirty: &mut [bool],
    ) {
        if tx0 >= tx1 || ty0 >= ty1 {
            return;
        }
        let range_w = (tx1 - tx0) as usize;
        let mut tiles = mem::take(&mut self.scratch_hashers);
        tiles.clear();
        tiles.resize(range_w * (ty1 - ty0) as usize, TILE_HASH_SEED);

        for item in self.display_list.items() {
            if !is_known_bounds(item.bounds) {
                continue;
            }
            let (itx0, ity0, itx1, ity1) = tile_range(item.bounds, tiles_x, tiles_y);
            let lo_x = itx0.max(tx0);
            let hi_x = itx1.min(tx1);
            let lo_y = ity0.max(ty0);
            let hi_y = ity1.min(ty1);
            if lo_x >= hi_x || lo_y >= hi_y {
                continue;
            }
            // Hashed once, then folded into each tile it covers, in draw order.
            let item_hash = item_hash(item);
            for ty in lo_y..hi_y {
                let row = (ty - ty0) as usize * range_w;
                for tile in &mut tiles[row + (lo_x - tx0) as usize..row + (hi_x - tx0) as usize] {
                    *tile = (tile.rotate_left(23) ^ item_hash).wrapping_mul(TILE_HASH_MUL);
                }
            }
        }

        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                let new_hash = tiles[(ty - ty0) as usize * range_w + (tx - tx0) as usize];
                let idx = (ty * tiles_x + tx) as usize;
                if new_hash != self.tile_hashes[idx] {
                    dirty[idx] = true;
                }
                self.tile_hashes[idx] = new_hash;
            }
        }
        self.scratch_hashers = tiles;
    }

    fn capture_prev_items(&mut self) {
        self.prev_items.clear();
        self.prev_items.extend(
            self.display_list
                .items()
                .iter()
                .map(|item| (item.cache_key, item.bounds)),
        );
    }

    fn resize_buffer(&mut self, info: &SurfaceInfo) -> AureaResult<()> {
        let (lw, lh) = (info.width, info.height);
        let scale = info.scale_factor.max(1.0);
        let (rw, rh) = info.pixel_size();
        let new_len = (rw as usize)
            .checked_mul(rh as usize)
            .ok_or(AureaError::RenderingFailed)?;
        self.frame_buffer
            .try_reserve(new_len.saturating_sub(self.frame_buffer.len()))
            .map_err(|_| AureaError::RenderingFailed)?;
        if let Some(compact) = smaller_buffer(self.frame_buffer.capacity(), new_len) {
            self.frame_buffer = compact;
        }
        self.frame_buffer.clear();
        self.frame_buffer.resize(new_len, 0);
        self.has_frame = false;
        self.logical_width = lw;
        self.logical_height = lh;
        self.width = rw;
        self.height = rh;
        self.scale_factor = scale;
        self.display_list.clear();
        self.pending_damage = None;
        self.prev_items.clear();
        self.tile_hashes.clear();
        self.last_frame_damage = None;
        Ok(())
    }
}

impl Renderer for CpuRasterizer {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.configure(info)
    }

    fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()> {
        self.resize_buffer(&info)
    }

    fn resize(&mut self, lw: u32, lh: u32) -> AureaResult<()> {
        self.resize_buffer(&SurfaceInfo::new(lw, lh, self.scale_factor))
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext + '_>> {
        self.display_list.clear();
        let mut ctx = RecordingContext::new(
            &mut self.display_list,
            self.logical_width,
            self.logical_height,
        );
        ctx.set_scale_factor(self.scale_factor);
        ctx.set_path_clips(true);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<FrameOutcome> {
        let (bw, bh) = (self.width, self.height);
        let pending = self.pending_damage.take();
        let diff = self.diff_damage();

        let Some(damage) = resolve_frame_damage(pending, diff, bw, bh) else {
            return Ok(FrameOutcome::Unchanged);
        };
        if damage.is_some_and(|r| {
            r.width <= 0.0
                || r.height <= 0.0
                || r.x >= bw as f32
                || r.y >= bh as f32
                || r.x + r.width <= 0.0
                || r.y + r.height <= 0.0
        }) {
            self.last_frame_damage = None;
            self.capture_prev_items();
            return Ok(FrameOutcome::Unchanged);
        }

        let (tiles_x, tiles_y) = tile_grid_dims(bw, bh);
        let dirty_tiles = self.compute_dirty_tiles(damage, pending, tiles_x, tiles_y);

        self.last_frame_damage = union_dirty_tile_rects(&dirty_tiles, tiles_x, tiles_y, bw, bh);
        if self.last_frame_damage.is_none() {
            self.scratch_dirty = dirty_tiles;
            self.capture_prev_items();
            return Ok(FrameOutcome::Unchanged);
        }

        let mut dirty_tiles = dirty_tiles;
        if let Some(region) = self.last_frame_damage {
            mark_tile_range_dirty(region, tiles_x, tiles_y, &mut dirty_tiles);
        }

        let surface_clip = match self.last_frame_damage {
            Some(region) => ClipBox::surface(bw, bh).intersect(region),
            None => ClipBox::surface(bw, bh),
        };
        let items = self.display_list.items();
        let mut occluded = mem::take(&mut self.scratch_occluded);
        let last_clear = mark_occluded(
            items,
            (tiles_x, tiles_y, bw, bh),
            &mut self.scratch_covered,
            &mut occluded,
            &mut self.scratch_under_clear,
        );
        for (i, item) in items.iter().enumerate() {
            if let DrawCommand::Clear(color) = &item.command {
                let (color, clip) = (color_to_u32(*color), surface_clip.intersect_opt(item.clip));
                let buf = &mut self.frame_buffer;
                if last_clear == Some(i) {
                    let dims = (tiles_x, tiles_y, bw, bh);
                    uncovered_runs(&self.scratch_under_clear, dims, |run| {
                        Self::clear_rect(&run, color, buf, bw, clip);
                    });
                } else if !occluded[i] {
                    let whole = Rect::new(0.0, 0.0, bw as f32, bh as f32);
                    Self::clear_rect(&whole, color, buf, bw, clip);
                }
                continue;
            }

            if !should_render_item(item, &occluded, i, &dirty_tiles, tiles_x, tiles_y) {
                continue;
            }
            Self::render_clipped_item(
                item,
                &mut self.frame_buffer,
                &mut self.scratch_path,
                &mut self.scratch_row,
                bw,
                surface_clip.intersect_opt(item.clip),
            )?;
        }

        self.scratch_occluded = occluded;
        self.scratch_dirty = dirty_tiles;
        self.capture_prev_items();

        self.has_frame = true;
        Ok(FrameOutcome::Presented)
    }

    fn frame_output(&self) -> Option<FrameOutput<'_>> {
        if !self.has_frame {
            return None;
        }
        FrameOutput::new(&self.frame_buffer, self.width, self.height)
    }

    fn last_frame_damage(&self) -> Option<Rect> {
        self.last_frame_damage
    }

    fn cleanup(&mut self) {
        self.display_list.clear();
        self.pending_damage = None;
        self.prev_items.clear();
        self.tile_hashes.clear();
    }

    fn set_damage(&mut self, damage: Option<Rect>) {
        let scale = self.scale_factor;
        self.pending_damage = damage.map(|rect| {
            Rect::new(
                rect.x * scale,
                rect.y * scale,
                rect.width * scale,
                rect.height * scale,
            )
        });
    }

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }
}

/// Per-circle values shared by every row of a stroked circle.
struct CircleStrokeCtx {
    clip: ClipBox,
    center: Point,
    radius: f32,
    inner_r: f32,
    c: u32,
    mode: BlendMode,
}

struct CircleFillCtx<'a> {
    clip: ClipBox,
    center: Point,
    radius: f32,
    paint: &'a Paint,
    mode: BlendMode,
    r_in: f32,
    r_out: f32,
    c_full: u32,
    opaque_fast: bool,
    const_src: Option<ConstSrc>,
}

#[derive(Default)]
struct ImageScratch {
    row: Vec<u32>,
    columns: Vec<(i32, f32)>,
    offsets: Vec<usize>,
    rows: Vec<(i32, f32)>,
    taps: Vec<sample::Taps>,
    /// Byte offset of each output column's source pixel, for nearest sampling.
    source_cols: Vec<usize>,
}

struct RectFillCtx<'a> {
    clip: ClipBox,
    paint: &'a Paint,
    mode: BlendMode,
    xl: f32,
    xr: f32,
    c_full: u32,
    opaque_fast: bool,
    const_src: Option<ConstSrc>,
}

fn on_pixel_grid(rect: &Rect) -> bool {
    [rect.x, rect.y, rect.right(), rect.bottom()]
        .iter()
        .all(|v| (v - v.round()).abs() < 1e-3)
}

fn rect_cov_x(x: u32, xl: f32, xr: f32) -> f32 {
    ((x as f32 + 1.0).min(xr) - (x as f32).max(xl)).clamp(0.0, 1.0)
}

fn rect_cov_y(y: u32, yl: f32, yr: f32) -> f32 {
    ((y as f32 + 1.0).min(yr) - (y as f32).max(yl)).clamp(0.0, 1.0)
}

fn bytes_fit(width: u32, height: u32, per_pixel: usize, len: usize) -> bool {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(per_pixel))
        .is_some_and(|need| len >= need)
}

fn scale_u8(v: u8, factor: f32) -> u8 {
    if factor >= 1.0 {
        return v;
    }
    f32_to_u8_clamped((f32::from(v) * factor).round())
}

fn fade_color(c: Color, alpha: f32) -> Color {
    if alpha >= 1.0 {
        return c;
    }
    Color {
        a: f32_to_u8_clamped((f32::from(c.a) * alpha).round()),
        ..c
    }
}

fn fade_paint(paint: &Paint, alpha: f32) -> Paint {
    if alpha >= 1.0 {
        return paint.clone();
    }
    let mut faded = paint.clone();
    faded.color = fade_color(paint.color, alpha);
    faded
}

fn fade_stops(stops: &[GradientStop], alpha: f32) -> Vec<GradientStop> {
    stops
        .iter()
        .map(|stop| GradientStop {
            color: fade_color(stop.color, alpha),
            ..*stop
        })
        .collect()
}

/// A gradient LUT colour, with the parts a Normal fill over an opaque pixel reads precomputed.
#[derive(Clone, Copy)]
struct GradientStep {
    color: u32,
    alpha: u8,
    /// Red, green and blue in 12-bit linear light, as [`LinearLuts`] stores them.
    linear: [u32; 3],
}

fn gradient_steps(lut: &[u32; 256], luts: LinearLuts) -> [GradientStep; 256] {
    lut.map(|color| {
        let [alpha, r, g, b] = color.to_be_bytes();
        GradientStep {
            color,
            alpha,
            linear: [luts.linear(r), luts.linear(g), luts.linear(b)],
        }
    })
}

/// Source-over of a gradient step onto `dst`. Opaque and transparent steps are exact; a translucent
/// step over an opaque pixel is mixed in 12-bit linear light, within one level of `blend_pixel`.
#[inline]
fn gradient_over(step: &GradientStep, dst: u32, luts: LinearLuts) -> u32 {
    match step.alpha {
        255 => step.color,
        0 => dst,
        alpha if dst >> 24 == 255 => {
            let [_, dr, dg, db] = dst.to_be_bytes();
            let [r, g, b] = step.linear;
            0xff00_0000
                | (luts.mix(r, dr, alpha) << 16)
                | (luts.mix(g, dg, alpha) << 8)
                | luts.mix(b, db, alpha)
        }
        _ => blend_pixel(step.color, dst, BlendMode::Normal),
    }
}

fn color_to_u32(c: Color) -> u32 {
    (u32::from(c.a) << 24) | (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
}

fn color_to_u32_with_coverage(c: Color, cov: f32) -> u32 {
    let a = f32_to_u32_clamped((f32::from(c.a) * cov).round());
    (a << 24) | (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
}

fn circle_coverage(center: Point, radius: f32, px: f32, py: f32) -> f32 {
    let cx = px + 0.5;
    let cy = py + 0.5;
    let d = ((cx - center.x).powi(2) + (cy - center.y).powi(2)).sqrt();
    if d >= radius + 0.5 {
        return 0.0;
    }
    if d <= radius - 0.5 {
        return 1.0;
    }
    (radius + 0.5 - d).clamp(0.0, 1.0)
}

#[cfg_attr(not(test), allow(dead_code))]
fn pixel_at(buf: &[u32], w: u32, x: u32, y: u32) -> u32 {
    let idx = (y * w + x) as usize;
    if idx < buf.len() { buf[idx] } else { 0 }
}

#[cfg(test)]
mod glyph_tests {
    use super::*;
    use crate::scene::display_list::DisplayIndex;
    use std::hint::black_box;
    use std::time::Instant;

    #[test]
    #[ignore = "manual CPU glyph clipping timing"]
    fn clipped_glyph_benchmark() {
        let mask = GlyphMask::new(256, 16384, vec![128; 256 * 16384 * 3].into());
        let mut pixels = vec![0u32; 256 * 256];
        let clip = ClipBox::surface(256, 256).intersect(Rect::new(0.0, 120.0, 256.0, 8.0));
        let start = Instant::now();
        for _ in 0..2000 {
            CpuRasterizer::draw_glyph(
                black_box(&mask),
                Point::new(0.0, -8192.0),
                Color::rgb(20, 30, 40),
                black_box(&mut pixels),
                256,
                clip,
            );
        }
        eprintln!(
            "clipped glyph: {:.2} us/draw",
            start.elapsed().as_secs_f64() * 1e6 / 2000.0
        );
        black_box(pixels);
    }

    #[test]
    fn glyph_color_alpha_scales_coverage() {
        let mask = GlyphMask::new(1, 1, vec![255; 3].into());
        for alpha in [0, 64, 128, 255] {
            let mut pixels = vec![0u32; 1];
            CpuRasterizer::draw_glyph(
                &mask,
                Point::new(0.0, 0.0),
                Color::rgba(255, 255, 255, alpha),
                &mut pixels,
                1,
                ClipBox::surface(1, 1),
            );
            assert_eq!(pixels[0] >> 24, u32::from(alpha));
        }
    }

    #[test]
    fn glyph_item_opacity_combines_with_color_alpha() {
        let item = DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(1),
            Rect::new(0.0, 0.0, 1.0, 1.0),
            false,
            BlendMode::Normal,
            DrawCommand::DrawGlyphMask(
                GlyphMask::new(1, 1, vec![255; 3].into()),
                Point::new(0.0, 0.0),
                Color::rgba(255, 255, 255, 128),
            ),
        )
        .with_opacity(0.5);
        let mut pixels = [0];
        CpuRasterizer::render_item(
            &item,
            &mut pixels,
            &mut PathScratch::default(),
            &mut ImageScratch::default(),
            1,
            ClipBox::surface(1, 1),
        )
        .expect("render");
        assert_eq!(pixels[0] >> 24, 64);
    }

    #[test]
    fn glyph_blends_respect_transparency_and_multiply() {
        let mask = GlyphMask::new(1, 1, vec![128, 64, 32].into());
        let mut transparent = [0];
        CpuRasterizer::draw_glyph(
            &mask,
            Point::new(0.0, 0.0),
            Color::rgb(255, 0, 0),
            &mut transparent,
            1,
            ClipBox::surface(1, 1),
        );
        assert_eq!(
            transparent[0], 0x80ff_0000,
            "straight RGB on a transparent layer"
        );
        let mask = GlyphMask::new(1, 1, vec![255; 3].into());
        let mut background = [0xff12_3456];
        CpuRasterizer::draw_glyph_blended(
            &mask,
            Point::new(0.0, 0.0),
            Color::rgb(255, 255, 255),
            BlendMode::Multiply,
            &mut background,
            1,
            ClipBox::surface(1, 1),
        );
        assert_eq!(
            background[0], 0xff12_3456,
            "white multiply preserves its backdrop"
        );
    }

    #[test]
    fn clipped_glyph_pixels_match_full_render() {
        let data: Vec<_> = (0..12 * 16 * 3)
            .map(|i| u8::try_from((i * 73) % 256).expect("byte"))
            .collect();
        let mask = GlyphMask::new(12, 16, data.into());
        for origin in [
            Point::new(0.0, 0.0),
            Point::new(-4.0, -7.0),
            Point::new(8.0, 10.0),
        ] {
            for alpha in [64, 255] {
                let mut full = vec![0xff12_3456; 20 * 20];
                let mut clipped = full.clone();
                let clip = ClipBox::surface(20, 20).intersect(Rect::new(3.0, 4.0, 11.0, 9.0));
                let color = Color::rgba(240, 120, 60, alpha);
                CpuRasterizer::draw_glyph(
                    &mask,
                    origin,
                    color,
                    &mut full,
                    20,
                    ClipBox::surface(20, 20),
                );
                CpuRasterizer::draw_glyph(&mask, origin, color, &mut clipped, 20, clip);
                for y in 0..20 {
                    for x in 0..20 {
                        let expected = if clip.contains(x, y) {
                            full[usize::try_from(y * 20 + x).expect("pixel index")]
                        } else {
                            0xff12_3456
                        };
                        assert_eq!(
                            clipped[usize::try_from(y * 20 + x).expect("pixel index")],
                            expected
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn far_offscreen_glyphs_do_not_overflow_clip_arithmetic() {
        let mask = GlyphMask::new(1, 1, vec![255; 3].into());
        for origin in [
            Point::new(f32::MIN, 0.0),
            Point::new(0.0, f32::MIN),
            Point::new(f32::MAX, f32::MAX),
        ] {
            let mut pixels = vec![0u32; 4];
            CpuRasterizer::draw_glyph(
                &mask,
                origin,
                Color::rgb(255, 255, 255),
                &mut pixels,
                2,
                ClipBox::surface(2, 2),
            );
            assert_eq!(pixels, [0; 4]);
        }
    }

    /// The per-pixel glyph blend before the fade table; the oracle for `draw_glyph_blended`.
    #[allow(clippy::too_many_arguments)]
    fn reference_glyph_blended(
        mask: &GlyphMask,
        origin: Point,
        color: Color,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        if mask.width == 0
            || mask.height == 0
            || color.a == 0
            || !bytes_fit(mask.width, mask.height, 3, mask.coverage.len())
        {
            return;
        }
        let luts = LinearLuts::get();
        let (tr, tg, tb) = (
            luts.linear(color.r),
            luts.linear(color.g),
            luts.linear(color.b),
        );
        let opaque_pixel = 0xFF00_0000
            | (u32::from(color.r) << 16)
            | (u32::from(color.g) << 8)
            | u32::from(color.b);
        let source = ConstSrc::new(opaque_pixel);
        let dx = f32_to_i32_clamped(origin.x.round());
        let dy = f32_to_i32_clamped(origin.y.round());

        let offset = |edge: u32, origin: i32, limit: u32| {
            usize::try_from((i64::from(edge) - i64::from(origin)).clamp(0, i64::from(limit)))
                .unwrap_or(0)
        };
        let x_lo = offset(clip.x0, dx, mask.width);
        let x_hi = offset(clip.x1, dx, mask.width);
        let y_lo = offset(clip.y0, dy, mask.height);
        let y_hi = offset(clip.y1, dy, mask.height);
        if x_lo >= x_hi || y_lo >= y_hi {
            return;
        }

        let px = usize::try_from(i64::from(dx) + x_lo as i64).unwrap_or(0);
        let py = usize::try_from(i64::from(dy) + y_lo as i64).unwrap_or(0);
        let alpha = u16::from(color.a);
        let fade =
            |coverage: u8| u8::try_from((u16::from(coverage) * alpha + 127) / 255).unwrap_or(255);
        for my in y_lo..y_hi {
            let row = my * mask.width as usize;
            let cov_row = &mask.coverage[(row + x_lo) * 3..(row + x_hi) * 3];
            let start = (py + my - y_lo) * bw as usize + px;
            let dst_row = &mut buf[start..start + x_hi - x_lo];

            for (coverage, dst) in cov_row.as_chunks::<3>().0.iter().zip(dst_row) {
                let (cr8, cg8, cb8) = if color.a == 255 {
                    (coverage[0], coverage[1], coverage[2])
                } else {
                    (fade(coverage[0]), fade(coverage[1]), fade(coverage[2]))
                };
                if cr8 == 0 && cg8 == 0 && cb8 == 0 {
                    continue;
                }

                if mode == BlendMode::Normal && cr8 == 255 && cg8 == 255 && cb8 == 255 {
                    *dst = opaque_pixel;
                    continue;
                }
                if mode != BlendMode::Normal || (*dst >> 24) != 255 {
                    let coverage = u32::from(cr8.max(cg8).max(cb8));
                    *dst = if mode == BlendMode::Normal {
                        source.over_at_alpha(*dst, coverage)
                    } else {
                        blend_pixel((coverage << 24) | (opaque_pixel & 0x00ff_ffff), *dst, mode)
                    };
                    continue;
                }
                let da = (*dst >> 24) & 0xff;
                let [_, dr, dg, db] = dst.to_be_bytes();
                let or_ = luts.mix(tr, dr, cr8);
                let og = luts.mix(tg, dg, cg8);
                let ob = luts.mix(tb, db, cb8);
                let sa = u32::from(cr8.max(cg8).max(cb8));
                let oa = sa + ((255 - sa) * da) / 255;
                *dst = (oa << 24) | (or_ << 16) | (og << 8) | ob;
            }
        }
    }

    #[test]
    fn glyph_blend_matches_per_pixel_reference() {
        let (gw, gh) = (37u32, 19u32);
        let coverage: Vec<u8> = (0..gw * gh * 3)
            .map(|i| u8::try_from((i * 73) % 256).expect("byte"))
            .collect();
        let mask = GlyphMask::new(gw, gh, coverage.into());
        let (sw, sh) = (40u32, 24u32);
        let clip = ClipBox::surface(sw, sh);
        let backdrop: Vec<u32> = (0..sw * sh)
            .map(|i| {
                let v = i.wrapping_mul(2_654_435_761);
                match i % 4 {
                    0 => 0,
                    1 => 0x8000_0000 | (v & 0x00ff_ffff),
                    2 => 0xff00_0000 | (v & 0x00ff_ffff),
                    _ => v,
                }
            })
            .collect();
        for alpha in [0, 1, 64, 128, 254, 255] {
            for color in [
                Color::rgba(200, 90, 17, alpha),
                Color::rgba(255, 255, 255, alpha),
            ] {
                for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
                    for origin in [Point::new(2.0, 3.0), Point::new(-5.0, 20.0)] {
                        let mut expected = backdrop.clone();
                        reference_glyph_blended(
                            &mask,
                            origin,
                            color,
                            mode,
                            &mut expected,
                            sw,
                            clip,
                        );
                        let mut actual = backdrop.clone();
                        CpuRasterizer::draw_glyph_blended(
                            &mask,
                            origin,
                            color,
                            mode,
                            &mut actual,
                            sw,
                            clip,
                        );
                        for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
                            assert_eq!(a, e, "alpha {alpha} {mode:?} {origin:?} pixel {i}");
                        }
                    }
                }
            }
        }
    }

    fn glyph_blend_bench(alpha: u8, label: &str) {
        const SIDE: u32 = 1024;
        const ITERS: u32 = 20;
        let coverage: Vec<u8> = (0..SIDE * SIDE * 3)
            .map(|i| u8::try_from((i * 73) % 256).expect("byte"))
            .collect();
        let mask = GlyphMask::new(SIDE, SIDE, coverage.into());
        let mut pixels = vec![0xff10_2030; (SIDE * SIDE) as usize];
        let clip = ClipBox::surface(SIDE, SIDE);
        let color = Color::rgba(20, 30, 40, alpha);
        let start = Instant::now();
        for _ in 0..ITERS {
            CpuRasterizer::draw_glyph_blended(
                black_box(&mask),
                Point::new(0.0, 0.0),
                color,
                BlendMode::Normal,
                black_box(&mut pixels),
                SIDE,
                clip,
            );
        }
        eprintln!(
            "{label}: {:.2} us/draw",
            start.elapsed().as_secs_f64() * 1e6 / f64::from(ITERS)
        );
        black_box(pixels);
    }

    #[test]
    #[ignore = "manual translucent glyph timing"]
    fn translucent_glyph_benchmark() {
        glyph_blend_bench(128, "translucent 1024x1024 glyph");
    }

    #[test]
    #[ignore = "manual opaque glyph timing"]
    fn opaque_glyph_benchmark() {
        glyph_blend_bench(255, "opaque 1024x1024 glyph");
    }
}

#[cfg(test)]
mod bad_source_tests {
    use super::*;

    const CLIP: ClipBox = ClipBox {
        x0: 0,
        y0: 0,
        x1: 8,
        y1: 8,
    };

    fn draw(image: &Image, src: Rect, sampling: Sampling) -> Vec<u32> {
        let mut buf = vec![0u32; 64];
        CpuRasterizer::draw_image(
            image,
            src,
            Rect::new(0.0, 0.0, 8.0, 8.0),
            sampling,
            BlendMode::Normal,
            1.0,
            &mut buf,
            &mut ImageScratch::default(),
            8,
            CLIP,
        );
        buf
    }

    #[test]
    fn empty_source_rects_and_images_draw_nothing() {
        let image = Image::new(2, 2, vec![255; 16]);
        for sampling in [Sampling::Linear, Sampling::Smooth, Sampling::Nearest] {
            for src in [
                Rect::new(0.0, 0.0, 0.0, 2.0),
                Rect::new(1.0, 1.0, -1.0, -1.0),
            ] {
                assert!(draw(&image, src, sampling).iter().all(|&p| p == 0));
            }
        }
        let empty = Image::new(0, 2, vec![255; 16]);
        let src = Rect::new(0.0, 0.0, 1.0, 1.0);
        assert!(draw(&empty, src, Sampling::Linear).iter().all(|&p| p == 0));
    }

    #[test]
    fn short_image_data_and_masks_are_skipped() {
        let short = Image::new(4, 4, vec![255; 8]);
        for sampling in [Sampling::Linear, Sampling::Smooth, Sampling::Nearest] {
            let src = Rect::new(0.0, 0.0, 2.0, 2.0);
            assert!(draw(&short, src, sampling).iter().all(|&p| p == 0));
        }
        let mask = GlyphMask::new(4, 4, vec![255; 6].into());
        let mut buf = vec![0u32; 64];
        let origin = Point::new(0.0, 0.0);
        CpuRasterizer::draw_glyph(&mask, origin, Color::rgb(0, 0, 0), &mut buf, 8, CLIP);
        assert!(buf.iter().all(|&p| p == 0));
    }
}

#[cfg(test)]
mod image_draw_tests {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    fn pattern(w: u32, h: u32, alpha: impl Fn(u32, u32) -> u8) -> Image {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = x.to_le_bytes()[0].wrapping_mul(37) ^ y.to_le_bytes()[0].wrapping_mul(91);
                data.extend([v, v.wrapping_mul(3), 255 - v, alpha(x, y)]);
            }
        }
        Image::new(w, h, data)
    }

    /// A per-pixel bilinear read.
    fn reference(image: &Image, src: Rect, dest: Rect, mode: BlendMode, alpha: f32) -> Vec<u32> {
        let mut buf = vec![0xff20_4060; 24 * 24];
        let b = sample::Bounds {
            x0: 0,
            y0: 0,
            x1: image.width as i32 - 1,
            y1: image.height as i32 - 1,
        };
        let (sx, sy) = (src.width / dest.width, src.height / dest.height);
        for y in 0..24 {
            for x in 0..24 {
                let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                if cx < dest.x || cy < dest.y || cx >= dest.right() || cy >= dest.bottom() {
                    continue;
                }
                let u = (x as f32 - dest.x) * sx + src.x + sx * 0.5;
                let v = (y as f32 - dest.y) * sy + src.y + sy * 0.5;
                let c = sample::bilinear(image, b, u, v, alpha);
                if c >> 24 != 0 {
                    CpuRasterizer::buf_set(&mut buf, 24, ClipBox::surface(24, 24), x, y, c, mode);
                }
            }
        }
        buf
    }

    #[test]
    fn enlarged_draws_stay_within_one_of_a_per_pixel_read() {
        let opaque = pattern(6, 5, |_, _| 255);
        let mixed = pattern(6, 5, |x, y| [255, 0, 128, 255, 7][((x + y) % 5) as usize]);
        let src = Rect::new(0.0, 0.0, 6.0, 5.0);
        let dest = Rect::new(1.3, 2.0, 21.0, 19.5);
        for image in [&opaque, &mixed] {
            for mode in [BlendMode::Normal, BlendMode::Multiply] {
                for alpha in [1.0, 0.5] {
                    let mut buf = vec![0xff20_4060; 24 * 24];
                    CpuRasterizer::draw_image(
                        image,
                        src,
                        dest,
                        Sampling::Linear,
                        mode,
                        alpha,
                        &mut buf,
                        &mut ImageScratch::default(),
                        24,
                        ClipBox::surface(24, 24),
                    );
                    let want = reference(image, src, dest, mode, alpha);
                    for (&got, &want) in buf.iter().zip(&want) {
                        let off =
                            [0, 8, 16, 24].map(|s| ((got >> s) & 255).abs_diff((want >> s) & 255));
                        assert!(
                            off.iter().all(|&d| d <= 1),
                            "{mode:?} {alpha}: {got:08x} {want:08x}"
                        );
                    }
                }
            }
        }
    }

    /// Lunia's zoom frame: a canvas-sized clear under ~80 enlarged opaque tiles.
    #[test]
    #[ignore = "manual CPU image draw timing"]
    fn zoomed_tiles_benchmark() {
        let (w, h) = (2732, 2048);
        let tile = pattern(256, 256, |_, _| 255);
        let mut r = CpuRasterizer::new(w, h);
        let frames = 20;
        let mut total = 0.0;
        for frame in 0..frames {
            let mut ctx = r.begin_frame().expect("frame");
            ctx.clear(Color::rgb(40, 40, 40)).expect("clear");
            let size = 256.0 * 1.3;
            let shift = frame as f32 * 1.7;
            for row in 0..8 {
                for col in 0..10 {
                    let dest = Rect::new(
                        col as f32 * size - shift,
                        row as f32 * size - shift,
                        size,
                        size,
                    );
                    ctx.draw_image_region(&tile, Rect::new(0.0, 0.0, 256.0, 256.0), dest)
                        .expect("draw");
                }
            }
            drop(ctx);
            let start = Instant::now();
            r.end_frame().expect("end");
            total += start.elapsed().as_secs_f64();
        }
        eprintln!(
            "zoomed tiles: {:.2} ms/frame",
            total * 1e3 / f64::from(frames)
        );
        black_box(r.pixels());
    }
}

#[cfg(test)]
mod stroke_rect_tests {
    use super::*;

    fn stroke_split(split: u32) -> Vec<u32> {
        let (w, h) = (20, 10);
        let mut buf = vec![0u32; (w * h) as usize];
        let rect = Rect::new(2.0, 2.0, 16.0, 6.0);
        let paint = Paint::new()
            .color(Color::rgb(255, 0, 0))
            .style(PaintStyle::Stroke)
            .stroke_width(1.0);
        for clip in [
            ClipBox {
                x0: 0,
                y0: 0,
                x1: split,
                y1: h,
            },
            ClipBox {
                x0: split,
                y0: 0,
                x1: w,
                y1: h,
            },
        ] {
            CpuRasterizer::draw_rect(&rect, &paint, BlendMode::Normal, &mut buf, w, clip);
        }
        buf
    }

    #[test]
    fn a_split_rectangle_has_no_edge_at_the_split() {
        let buf = stroke_split(10);
        let at = |x: u32, y: u32| buf[(y * 20 + x) as usize];
        assert_eq!(at(9, 4), 0);
        assert_eq!(at(10, 4), 0);
        assert_ne!(at(2, 4), 0);
        assert_ne!(at(17, 4), 0);
        assert_ne!(at(9, 2), 0);
        assert_ne!(at(10, 7), 0);
    }

    #[test]
    fn splitting_does_not_change_the_outline() {
        assert_eq!(stroke_split(10), stroke_split(20));
    }
}

#[cfg(test)]
mod diff_damage_tests {
    use super::*;
    use crate::scene::command::DrawCommand;
    use crate::scene::display_list::{DisplayIndex, DisplayItem};

    fn item(key: u64, bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(key),
            bounds,
            false,
            BlendMode::Normal,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    #[test]
    fn identical_list_reports_unchanged() {
        let mut r = CpuRasterizer::new(100, 100);
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        r.display_list.push(item(1, bounds));
        r.prev_items.push((CacheKey::from_hash(1), bounds));

        assert!(matches!(r.diff_damage(), FrameDamage::Unchanged));
    }

    #[test]
    fn a_middle_that_changes_length_damages_only_the_middle() {
        let mut r = CpuRasterizer::new(100, 100);
        let rect = |x: f32| Rect::new(x, 0.0, 10.0, 10.0);
        for (key, x) in [(1, 0.0), (2, 20.0), (3, 40.0), (9, 80.0)] {
            r.prev_items.push((CacheKey::from_hash(key), rect(x)));
        }
        for (key, x) in [(1, 0.0), (4, 30.0), (9, 80.0)] {
            r.display_list.push(item(key, rect(x)));
        }
        let FrameDamage::Region(damage) = r.diff_damage() else {
            panic!("expected a region");
        };
        assert_eq!(
            damage,
            Rect::new(20.0, 0.0, 30.0, 10.0),
            "the panel after stays clean"
        );
    }

    #[test]
    fn offscreen_changes_skip_presentation_and_update_previous_items() {
        let mut r = CpuRasterizer::new(32, 32);
        for step in 0..3 {
            {
                let mut ctx = r.begin_frame().expect("begin");
                ctx.clear(Color::rgb(10, 20, 30)).expect("clear");
                ctx.draw_rect(Rect::new(64.0 + step as f32, 4.0, 2.0, 2.0), &Paint::new())
                    .expect("offscreen rect");
            }
            let expected = if step == 0 {
                FrameOutcome::Presented
            } else {
                FrameOutcome::Unchanged
            };
            assert_eq!(r.end_frame().expect("end"), expected);
            assert!(r.pixels().iter().all(|&pixel| pixel == 0xff0a_141e));
            assert!(matches!(r.diff_damage(), FrameDamage::Unchanged));
        }
    }

    #[test]
    fn changed_item_unions_old_and_new_bounds() {
        let mut r = CpuRasterizer::new(100, 100);
        let old_bounds = Rect::new(0.0, 0.0, 8.0, 8.0);
        let new_bounds = Rect::new(20.0, 20.0, 8.0, 8.0);
        r.display_list.push(item(2, new_bounds));
        r.prev_items.push((CacheKey::from_hash(1), old_bounds));

        match r.diff_damage() {
            FrameDamage::Region(rect) => assert_eq!(rect, Rect::new(0.0, 0.0, 28.0, 28.0)),
            other => panic!("expected Region, got {other:?}"),
        }
    }

    #[test]
    fn unknown_bounds_change_forces_full_damage() {
        let mut r = CpuRasterizer::new(100, 100);
        let unknown = Rect::new(0.0, 0.0, 0.0, 0.0);
        r.display_list.push(item(2, unknown));
        r.prev_items.push((CacheKey::from_hash(1), unknown));

        assert!(matches!(r.diff_damage(), FrameDamage::Full));
    }

    #[test]
    fn appended_item_contributes_only_its_own_bounds() {
        let mut r = CpuRasterizer::new(100, 100);
        let shared_bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let new_bounds = Rect::new(50.0, 50.0, 4.0, 4.0);
        r.display_list.push(item(1, shared_bounds));
        r.display_list.push(item(2, new_bounds));
        r.prev_items.push((CacheKey::from_hash(1), shared_bounds));

        match r.diff_damage() {
            FrameDamage::Region(rect) => assert_eq!(rect, new_bounds),
            other => panic!("expected Region, got {other:?}"),
        }
    }

    #[test]
    fn static_scene_skips_present_on_repeat_frame() {
        use crate::types::Paint;

        let mut r = CpuRasterizer::new(32, 32);
        let paint = Paint::new();

        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(10, 20, 30)).unwrap();
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &paint)
            .unwrap();
        drop(ctx);
        assert_eq!(r.end_frame().unwrap(), FrameOutcome::Presented);
        assert!(r.frame_output().is_some());

        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(10, 20, 30)).unwrap();
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &paint)
            .unwrap();
        drop(ctx);

        assert_eq!(r.end_frame().unwrap(), FrameOutcome::Unchanged);
        assert!(r.frame_output().is_some());
    }

    #[test]
    fn each_renderer_publishes_only_its_own_frame() {
        use crate::types::Paint;

        let mut first = CpuRasterizer::new(16, 16);
        {
            let mut ctx = first.begin_frame().unwrap();
            ctx.clear(Color::rgb(255, 0, 0)).unwrap();
            ctx.draw_rect(Rect::new(1.0, 1.0, 4.0, 4.0), &Paint::new())
                .unwrap();
        }
        first.end_frame().unwrap();

        let mut second = CpuRasterizer::new(32, 32);
        {
            let mut ctx = second.begin_frame().unwrap();
            ctx.clear(Color::rgb(0, 0, 255)).unwrap();
            ctx.draw_rect(Rect::new(2.0, 2.0, 8.0, 8.0), &Paint::new())
                .unwrap();
        }
        second.end_frame().unwrap();

        let a = first.frame_output().expect("first drew a frame");
        let b = second.frame_output().expect("second drew a frame");

        assert_eq!((a.width, a.height), (16, 16), "the first renderer's own");
        assert_eq!((b.width, b.height), (32, 32), "and the second's own");
        assert_ne!(a.pixels(), b.pixels(), "two buffers, not one shared slot");
    }

    #[test]
    fn a_resized_renderer_has_no_frame_to_publish() {
        let mut r = CpuRasterizer::new(16, 16);
        {
            let mut ctx = r.begin_frame().unwrap();
            ctx.clear(Color::rgb(255, 0, 0)).unwrap();
        }
        r.end_frame().unwrap();
        assert!(r.frame_output().is_some());

        r.resize(32, 32).unwrap();

        assert!(
            r.frame_output().is_none(),
            "an empty buffer of the new size is not a frame"
        );
    }

    #[test]
    fn reinitialization_reuses_storage_and_invalidates_old_frame_state() {
        let mut r = CpuRasterizer::new(32, 32);
        {
            r.begin_frame()
                .expect("begin")
                .clear(Color::rgb(255, 0, 0))
                .expect("clear");
        }
        r.end_frame().expect("end");
        r.set_damage(Some(Rect::new(4.0, 4.0, 1.0, 1.0)));
        let storage = r.frame_buffer.as_ptr();
        r.init(
            Surface::Cpu,
            SurfaceInfo {
                width: 16,
                height: 16,
                scale_factor: 1.0,
                pixels: None,
            },
        )
        .expect("reinit");
        assert_eq!(r.frame_buffer.as_ptr(), storage);
        assert!(r.frame_output().is_none());
        assert!(r.display_list.items().is_empty());
        assert!(r.pending_damage.is_none());
        assert!(r.pixels().iter().all(|&pixel| pixel == 0));
    }

    #[test]
    fn substantial_window_shrink_releases_oversized_framebuffer_capacity() {
        let mut r = CpuRasterizer::new(2048, 1024);
        let large_capacity = r.frame_buffer.capacity();
        r.resize(128, 128).expect("shrink");
        assert_eq!(r.pixels().len(), 128 * 128);
        assert!(r.frame_buffer.capacity() <= 2 * r.pixels().len());
        assert!(large_capacity - r.frame_buffer.capacity() >= (1 << 20));
        let small_storage = r.frame_buffer.as_ptr();
        r.resize(120, 120).expect("small resize");
        assert_eq!(
            r.frame_buffer.as_ptr(),
            small_storage,
            "modest changes retain useful slack"
        );
    }
}

#[cfg(test)]
mod occlusion_tests {
    use super::*;
    use crate::scene::command::DrawCommand;
    use crate::scene::display_list::{DisplayIndex, DisplayItem};

    const W: u32 = TILE_SIZE * 4;
    const H: u32 = TILE_SIZE * 2;
    const TILES_X: u32 = 4;
    const TILES_Y: u32 = 2;

    fn occluder_item(bounds: Rect, blend: BlendMode) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(1),
            bounds,
            true,
            blend,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    fn plain_item(bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(2),
            bounds,
            false,
            BlendMode::Normal,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    fn occluded(items: &[DisplayItem]) -> Vec<bool> {
        let (mut covered, mut occluded, mut under) = (Vec::new(), Vec::new(), Vec::new());
        mark_occluded(
            items,
            (TILES_X, TILES_Y, W, H),
            &mut covered,
            &mut occluded,
            &mut under,
        );
        occluded
    }

    fn image_item(image: &Image, src: Rect, bounds: Rect) -> DisplayItem {
        let mut list = DisplayList::new();
        let mut ctx = RecordingContext::new(&mut list, W, H);
        ctx.draw_image_region(image, src, bounds).expect("draw");
        drop(ctx);
        list.items()[0].clone()
    }

    #[test]
    fn opaque_images_hide_what_is_under_them() {
        let solid = Image::new(4, 4, vec![255; 64]);
        let mut see_through = vec![255; 64];
        see_through[63] = 254;
        let see_through = Image::new(4, 4, see_through);
        let all = Rect::new(0.0, 0.0, 4.0, 4.0);
        let hides = |image: &Image, src: Rect| {
            occluded(&[plain_item(first_tile()), image_item(image, src, whole())])[0]
        };
        assert!(hides(&solid, all));
        assert!(hides(&solid, Rect::new(1.0, 1.0, 2.0, 2.0)));
        assert!(!hides(&see_through, all));
        assert!(
            !hides(&solid, Rect::new(-1.0, 0.0, 4.0, 4.0)),
            "reaches past the image"
        );
    }

    #[test]
    fn the_last_clear_learns_what_later_draws_cover() {
        let left = Rect::new(0.0, 0.0, TILE_SIZE as f32, H as f32);
        let items = vec![
            plain_item(first_tile()),
            occluder_item(Rect::new(0.0, 0.0, f32::MAX, f32::MAX), BlendMode::Normal),
            occluder_item(left, BlendMode::Normal),
        ];
        let (mut covered, mut occluded, mut under) = (Vec::new(), Vec::new(), Vec::new());
        let dims = (TILES_X, TILES_Y, W, H);
        let last = mark_occluded(&items, dims, &mut covered, &mut occluded, &mut under);
        assert_eq!(last, Some(2), "the covering draw is itself a clear here");
        let mut runs = Vec::new();
        uncovered_runs(
            &[true, false, false, true, false, true, true, false],
            dims,
            |r| {
                runs.push((r.x, r.y, r.width));
            },
        );
        let t = TILE_SIZE as f32;
        assert_eq!(runs, [(t, 0.0, 2.0 * t), (0.0, t, t), (3.0 * t, t, t)]);
    }

    #[test]
    fn a_clear_under_an_opaque_image_still_shows_around_it() {
        let mut r = CpuRasterizer::new(W, H);
        let image = Image::new(1, 1, vec![10, 20, 30, 255]);
        let pixels = frame_of(&mut r, |ctx| {
            ctx.clear(Color::rgb(200, 0, 0)).unwrap();
            ctx.draw_image_rect(&image, Rect::new(0.0, 0.0, (W / 2) as f32, H as f32))
                .unwrap();
        });
        assert_eq!(pixels[0], 0xff0a_141e);
        assert_eq!(pixels[(W - 1) as usize], 0xffc8_0000);
        assert_eq!(pixels[(W * H - 1) as usize], 0xffc8_0000);
    }

    fn whole() -> Rect {
        Rect::new(0.0, 0.0, W as f32, H as f32)
    }
    fn first_tile() -> Rect {
        Rect::new(0.0, 0.0, TILE_SIZE as f32, TILE_SIZE as f32)
    }

    #[test]
    fn rect_contains_basic() {
        let outer = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(rect_contains(outer, Rect::new(1.0, 1.0, 5.0, 5.0)));
        assert!(rect_contains(outer, outer));
        assert!(!rect_contains(outer, Rect::new(5.0, 5.0, 10.0, 10.0)));
        assert!(!rect_contains(outer, Rect::new(-1.0, 0.0, 5.0, 5.0)));
    }

    #[test]
    fn a_later_opaque_draw_hides_what_is_under_it() {
        let items = vec![
            plain_item(first_tile()),
            occluder_item(whole(), BlendMode::Normal),
        ];
        assert!(occluded(&items)[0]);
    }

    #[test]
    fn an_earlier_draw_does_not_hide_a_later_one() {
        let items = vec![
            occluder_item(whole(), BlendMode::Normal),
            plain_item(first_tile()),
        ];
        assert!(!occluded(&items)[1]);
    }

    #[test]
    fn a_draw_that_covers_only_part_hides_nothing_outside_it() {
        let last_tile = Rect::new(
            (TILE_SIZE * 3) as f32,
            0.0,
            TILE_SIZE as f32,
            TILE_SIZE as f32,
        );
        let left = Rect::new(0.0, 0.0, TILE_SIZE as f32, H as f32);
        let items = vec![
            plain_item(last_tile),
            occluder_item(left, BlendMode::Normal),
        ];
        assert!(!occluded(&items)[0]);
    }

    #[test]
    fn a_blend_that_is_not_normal_hides_nothing() {
        let items = vec![
            plain_item(first_tile()),
            occluder_item(whole(), BlendMode::Multiply),
        ];
        assert!(!occluded(&items)[0]);
    }

    #[test]
    fn a_draw_that_is_not_opaque_hides_nothing() {
        let items = vec![plain_item(first_tile()), plain_item(whole())];
        assert!(!occluded(&items)[0]);
    }

    #[test]
    fn a_clipped_draw_hides_only_what_its_clip_reaches() {
        let last_tile = Rect::new(
            (TILE_SIZE * 3) as f32,
            0.0,
            TILE_SIZE as f32,
            TILE_SIZE as f32,
        );
        let mut occluder = occluder_item(whole(), BlendMode::Normal);
        occluder.clip = Some(Rect::new(0.0, 0.0, TILE_SIZE as f32, H as f32));
        let items = vec![plain_item(last_tile), occluder];

        assert!(
            !occluded(&items)[0],
            "the draw is clipped to the first tile column and cannot hide the last"
        );
    }

    #[test]
    fn a_clip_that_still_covers_the_item_hides_it() {
        let mut occluder = occluder_item(whole(), BlendMode::Normal);
        occluder.clip = Some(Rect::new(0.0, 0.0, (TILE_SIZE * 2) as f32, H as f32));
        let items = vec![plain_item(first_tile()), occluder];

        assert!(occluded(&items)[0]);
    }

    #[test]
    fn a_circle_covers_less_than_the_square_around_it() {
        let radius = TILE_SIZE as f32;
        let center = Point::new(radius, radius);
        let square = inscribed_square(center, radius);

        assert!(square.width < radius * 2.0, "smaller than the bounds");
        assert!(
            (center.x - square.x).hypot(center.y - square.y) <= radius + 0.001,
            "and its corner is still inside the circle"
        );
    }

    #[test]
    fn draws_combine_to_hide_what_neither_covers_alone() {
        let across = Rect::new(0.0, 0.0, (TILE_SIZE * 2) as f32, H as f32);
        let left = Rect::new(0.0, 0.0, TILE_SIZE as f32, H as f32);
        let right = Rect::new(TILE_SIZE as f32, 0.0, TILE_SIZE as f32, H as f32);
        let items = vec![
            plain_item(across),
            occluder_item(left, BlendMode::Normal),
            occluder_item(right, BlendMode::Normal),
        ];

        assert!(occluded(&items)[0]);
    }

    #[test]
    fn a_draw_ending_inside_a_tile_does_not_claim_it() {
        let half = Rect::new(0.0, 0.0, (TILE_SIZE / 2) as f32, H as f32);
        let items = vec![
            plain_item(first_tile()),
            occluder_item(half, BlendMode::Normal),
        ];

        assert!(!occluded(&items)[0], "half a tile is not a covered tile");
    }

    fn frame_of(r: &mut CpuRasterizer, scene: impl FnOnce(&mut dyn DrawingContext)) -> Vec<u32> {
        let mut ctx = r.begin_frame().unwrap();
        scene(&mut *ctx);
        drop(ctx);
        r.end_frame().unwrap();
        r.frame_buffer.clone()
    }

    #[test]
    fn repeated_frames_of_the_same_scene_are_stable() {
        let mut r = CpuRasterizer::new(240, 160);
        let scene = |caret_x: f32| {
            move |ctx: &mut dyn DrawingContext| {
                ctx.clear(Color::rgb(16, 18, 22)).unwrap();
                let panel = Paint::new().color(Color::rgb(30, 34, 40));
                ctx.draw_rect(Rect::new(0.0, 0.0, 240.0, 40.0), &panel)
                    .unwrap();
                let text = Paint::new().color(Color::rgb(200, 200, 210));
                for row in 0..6u8 {
                    let y = 50.0 + f32::from(row) * 14.0;
                    ctx.draw_rect(Rect::new(8.0, y, 180.0, 8.0), &text).unwrap();
                }
                let caret = Paint::new().color(Color::rgb(255, 220, 120));
                ctx.draw_rect(Rect::new(caret_x, 50.0, 2.0, 84.0), &caret)
                    .unwrap();
            }
        };
        frame_of(&mut r, scene(20.0));
        let settled = frame_of(&mut r, scene(20.0));
        assert_eq!(
            settled,
            frame_of(&mut r, scene(20.0)),
            "an unchanged scene changed"
        );
        frame_of(&mut r, scene(60.0));
        assert_eq!(
            settled,
            frame_of(&mut r, scene(20.0)),
            "the caret left a mark"
        );
    }

    #[test]
    fn content_returns_when_an_overlay_is_dismissed() {
        let mut r = CpuRasterizer::new(240, 160);
        let scene = |overlay: bool| {
            move |ctx: &mut dyn DrawingContext| {
                ctx.clear(Color::rgb(16, 18, 22)).unwrap();
                let text = Paint::new().color(Color::rgb(200, 200, 210));
                for row in 0..8u8 {
                    let y = 20.0 + f32::from(row) * 14.0;
                    ctx.draw_rect(Rect::new(8.0, y, 200.0, 8.0), &text).unwrap();
                }
                if overlay {
                    let panel = Paint::new().color(Color::rgb(40, 44, 52));
                    ctx.draw_rect(Rect::new(0.0, 0.0, 240.0, 160.0), &panel)
                        .unwrap();
                }
            }
        };
        frame_of(&mut r, scene(false));
        let without = frame_of(&mut r, scene(false));
        frame_of(&mut r, scene(true));
        assert_eq!(
            without,
            frame_of(&mut r, scene(false)),
            "covered content did not return"
        );
    }
}

#[cfg(test)]
mod tile_cache_tests {
    use super::*;
    use crate::scene::command::DrawCommand;
    use crate::scene::display_list::{DisplayIndex, DisplayItem};
    use crate::types::Paint;

    fn tile() -> f32 {
        TILE_SIZE as f32
    }

    fn two_by_two() -> CpuRasterizer {
        CpuRasterizer::new(TILE_SIZE * 2, TILE_SIZE * 2)
    }

    fn whole() -> Rect {
        Rect::new(0.0, 0.0, tile() * 2.0, tile() * 2.0)
    }

    fn inside(tx: f32, ty: f32) -> Rect {
        Rect::new(tx * tile() + 4.0, ty * tile() + 4.0, 8.0, 8.0)
    }

    fn item(key: u64, bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(key),
            bounds,
            false,
            BlendMode::Normal,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    #[test]
    fn tile_grid_dims_rounds_up() {
        assert_eq!(tile_grid_dims(TILE_SIZE, TILE_SIZE), (1, 1));
        assert_eq!(tile_grid_dims(TILE_SIZE + 1, TILE_SIZE), (2, 1));
        assert_eq!(tile_grid_dims(TILE_SIZE * 2, TILE_SIZE + 44), (2, 2));
    }

    #[test]
    fn tile_range_clamps_huge_bounds_to_grid() {
        let max = Rect::new(0.0, 0.0, f32::MAX, f32::MAX);
        assert_eq!(tile_range(max, 3, 2), (0, 0, 3, 2));
    }

    #[test]
    fn tile_range_picks_single_tile() {
        let bounds = Rect::new(tile() + 10.0, 10.0, 4.0, 4.0);
        assert_eq!(tile_range(bounds, 4, 4), (1, 0, 2, 1));
    }

    #[test]
    fn changing_one_tiles_item_leaves_other_tile_clean() {
        let mut r = two_by_two();
        let a = inside(0.0, 0.0);
        let b = inside(1.0, 1.0);
        let full = whole();

        r.display_list.push(item(1, a));
        r.display_list.push(item(2, b));
        let dirty = r.compute_dirty_tiles(Some(full), None, 2, 2);
        assert!(dirty[0], "tile (0,0) dirty on first computation");
        assert!(dirty[3], "tile (1,1) dirty on first computation");

        r.display_list.clear();
        r.display_list.push(item(10, a));
        r.display_list.push(item(2, b));
        let dirty = r.compute_dirty_tiles(Some(full), None, 2, 2);
        assert!(dirty[0], "tile (0,0) dirty: its item's cache key changed");
        assert!(!dirty[1], "tile (1,0) is empty and unchanged");
        assert!(!dirty[2], "tile (0,1) is empty and unchanged");
        assert!(!dirty[3], "tile (1,1) clean: its item is unchanged");
    }

    #[test]
    fn forced_damage_marks_tile_dirty_regardless_of_hash() {
        let mut r = two_by_two();
        let a = inside(0.0, 0.0);
        let forced = inside(1.0, 1.0);
        let full = whole();

        r.display_list.push(item(1, a));
        let _ = r.compute_dirty_tiles(Some(full), None, 2, 2);

        let dirty = r.compute_dirty_tiles(Some(full), Some(forced), 2, 2);
        assert!(!dirty[0], "tile (0,0) unchanged and not forced");
        assert!(dirty[3], "tile (1,1) forced dirty by set_damage");
    }

    #[test]
    fn a_spanning_item_does_not_drag_in_its_clean_tiles() {
        let mut r = two_by_two();
        let full = whole();
        let spanning = Rect::new(tile() - 16.0, 4.0, 40.0, 8.0);
        let small = inside(0.0, 0.0);

        r.display_list.push(item(100, spanning));
        r.display_list.push(item(1, small));
        let _ = r.compute_dirty_tiles(Some(full), None, 2, 2);

        r.display_list.clear();
        r.display_list.push(item(100, spanning));
        r.display_list.push(item(2, small));
        let dirty = r.compute_dirty_tiles(Some(full), None, 2, 2);

        assert!(dirty[0], "tile (0,0) dirty: small's cache key changed");
        assert!(
            !dirty[1],
            "tile (1,0) stays clean: the spanning item is clipped to the repaint"
        );
        assert!(!dirty[2], "tile (0,1) untouched");
        assert!(!dirty[3], "tile (1,1) untouched");
    }

    #[test]
    fn a_clean_tile_inside_the_repaint_keeps_its_items() {
        let mut r = CpuRasterizer::new(512, 512);
        let background = Paint::new().color(Color::rgb(1, 2, 3));
        let kept = Paint::new().color(Color::rgb(200, 100, 50));
        let frame = |r: &mut CpuRasterizer, corner: Color| {
            let corner = Paint::new().color(corner);
            let mut ctx = r.begin_frame().unwrap();
            ctx.draw_rect(Rect::new(0.0, 0.0, 512.0, 512.0), &background)
                .unwrap();
            ctx.draw_rect(Rect::new(4.0, 4.0, 8.0, 8.0), &corner)
                .unwrap();
            ctx.draw_rect(Rect::new(300.0, 4.0, 8.0, 8.0), &kept)
                .unwrap();
            ctx.draw_rect(Rect::new(300.0, 300.0, 8.0, 8.0), &corner)
                .unwrap();
            drop(ctx);
            r.end_frame().unwrap();
        };
        frame(&mut r, Color::rgb(10, 20, 30));
        frame(&mut r, Color::rgb(0, 255, 0));

        let bw = r.width;
        assert_eq!(
            pixel_at(&r.frame_buffer, bw, 8, 8),
            color_to_u32(Color::rgb(0, 255, 0))
        );
        assert_eq!(
            pixel_at(&r.frame_buffer, bw, 304, 8),
            color_to_u32(kept.color)
        );
    }

    #[test]
    fn unrelated_tile_pixels_survive_a_localized_redraw() {
        let mut r = CpuRasterizer::new(512, 512);
        let paint_a = Paint::new().color(Color::rgb(10, 20, 30));
        let paint_b = Paint::new().color(Color::rgb(200, 100, 50));
        let paint_c = Paint::new().color(Color::rgb(0, 255, 0));

        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(1, 2, 3)).unwrap();
        ctx.draw_rect(Rect::new(4.0, 4.0, 8.0, 8.0), &paint_a)
            .unwrap();
        ctx.draw_rect(Rect::new(300.0, 300.0, 8.0, 8.0), &paint_b)
            .unwrap();
        drop(ctx);
        r.end_frame().unwrap();

        let bw = r.width;
        assert_eq!(
            pixel_at(&r.frame_buffer, bw, 304, 304),
            color_to_u32(paint_b.color)
        );

        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(1, 2, 3)).unwrap();
        ctx.draw_rect(Rect::new(4.0, 4.0, 8.0, 8.0), &paint_c)
            .unwrap();
        ctx.draw_rect(Rect::new(300.0, 300.0, 8.0, 8.0), &paint_b)
            .unwrap();
        drop(ctx);
        r.end_frame().unwrap();

        assert_eq!(
            pixel_at(&r.frame_buffer, bw, 8, 8),
            color_to_u32(paint_c.color)
        );
        assert_eq!(
            pixel_at(&r.frame_buffer, bw, 304, 304),
            color_to_u32(paint_b.color)
        );
    }

    #[test]
    fn an_image_blit_adds_nothing_of_its_own() {
        let size = 170u32;
        let last = size - 1;
        let mut data = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let v = u8::try_from(255 - y * 255 / last).expect("fits");
                let s = u8::try_from(x * 255 / last).expect("fits");
                let min = u8::try_from(u32::from(v) * (255 - u32::from(s)) / 255).expect("fits");
                data.extend_from_slice(&[v, min, min, 255]);
            }
        }
        let image = Image::new(size, size, data);
        let dest = Rect::new(33.4, 57.6, 170.0, 170.0);
        let card = Paint::new().color(Color::rgb(40, 44, 52));

        let mut r = CpuRasterizer::new(512, 512);
        for _ in 0..2 {
            let mut ctx = r.begin_frame().unwrap();
            ctx.draw_rect(Rect::new(0.0, 0.0, 512.0, 512.0), &card)
                .unwrap();
            ctx.draw_image_rect(&image, dest).unwrap();
            drop(ctx);
            r.end_frame().unwrap();
        }

        let bw = r.width;
        for y in 131..169u32 {
            for x in 1..40u32 {
                let px = pixel_at(&r.frame_buffer, bw, 34 + x, 58 + y);
                let brightest = [(px >> 16) & 0xFF, (px >> 8) & 0xFF, px & 0xFF]
                    .into_iter()
                    .max()
                    .unwrap_or(0);
                assert!(brightest < 60, "pixel ({x}, {y}) of the image is {px:08x}");
            }
        }
    }
}

#[cfg(test)]
mod scale_tests {
    use super::*;
    use crate::geometry::Transform;

    fn frame_at(scale: f32, draw: impl FnOnce(&mut dyn DrawingContext)) -> CpuRasterizer {
        let mut r = CpuRasterizer::new(40, 40);
        r.init(
            Surface::Cpu,
            SurfaceInfo {
                width: 40,
                height: 40,
                scale_factor: scale,
                pixels: None,
            },
        )
        .unwrap();
        let mut ctx = r.begin_frame().unwrap();
        draw(&mut *ctx);
        drop(ctx);
        r.end_frame().unwrap();
        r
    }

    #[test]
    fn logical_rects_land_on_their_physical_pixels() {
        let red = Color::rgb(255, 0, 0);
        for scale in [1.25f32, 1.5, 2.0] {
            let r = frame_at(scale, |ctx| {
                ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                ctx.draw_rect(Rect::new(10.0, 10.0, 20.0, 20.0), &Paint::new().color(red))
                    .unwrap();
            });
            let (_, _, w, h) = r.get_buffer();
            let side = f32_to_u32_clamped((40.0 * scale).round());
            assert_eq!((w, h), (side, side), "buffer at {scale}");
            let at = |logical: f32| f32_to_u32_clamped((logical * scale).floor());
            let buf = r.pixels();
            assert_eq!(
                pixel_at(buf, w, at(10.5), at(10.5)),
                color_to_u32(red),
                "inside at {scale}"
            );
            assert_eq!(
                pixel_at(buf, w, at(29.5), at(29.5)),
                color_to_u32(red),
                "far corner at {scale}"
            );
            assert_ne!(
                pixel_at(buf, w, at(9.0), at(9.0)),
                color_to_u32(red),
                "outside at {scale}"
            );
            assert_ne!(
                pixel_at(buf, w, at(31.0), at(31.0)),
                color_to_u32(red),
                "past it at {scale}"
            );
        }
    }

    #[test]
    fn a_stroke_scales_its_width_with_the_display() {
        for scale in [1.25f32, 1.5, 2.0] {
            let r = frame_at(scale, |ctx| {
                ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                let paint = Paint::new()
                    .color(Color::rgb(255, 255, 255))
                    .style(PaintStyle::Stroke)
                    .stroke_width(4.0);
                ctx.draw_line(0.0, 20.0, 40.0, 20.0, &paint).unwrap();
            });
            let (_, _, w, h) = r.get_buffer();
            let column: Vec<u32> = (0..h).map(|y| pixel_at(r.pixels(), w, w / 2, y)).collect();
            let lit = column.iter().filter(|&&p| p & 0xFF > 0x80).count();
            let want = 4.0 * scale;
            #[allow(clippy::cast_precision_loss)]
            let got = lit as f32;
            assert!(
                (got - want).abs() <= 1.0,
                "a 4px line is {want} device pixels at {scale}, got {got}"
            );
        }
    }

    #[test]
    fn snapped_borders_have_no_soft_edges() {
        let white = Color::rgb(255, 255, 255);
        for scale in [1.25f32, 1.5, 1.75] {
            for snap in [false, true] {
                let r = frame_at(scale, |ctx| {
                    ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                    ctx.set_pixel_snap(snap).unwrap();
                    let line = Paint::new()
                        .color(white)
                        .style(PaintStyle::Stroke)
                        .stroke_width(1.0);
                    ctx.draw_line(0.0, 10.3, 40.0, 10.3, &line).unwrap();
                    ctx.draw_rect(Rect::new(5.2, 15.1, 20.0, 12.0), &line)
                        .unwrap();
                    let hair = Paint::new().color(white);
                    ctx.draw_rect(Rect::new(0.0, 33.3, 40.0, 0.4), &hair)
                        .unwrap();
                });
                let (_, _, w, h) = r.get_buffer();
                let column: Vec<u32> = (0..h)
                    .map(|y| pixel_at(r.pixels(), w, f32_to_u32_clamped(2.5 * scale), y) & 0xFF)
                    .collect();
                let soft = column.iter().filter(|&&v| v > 8 && v < 247).count();
                let lit = column.iter().filter(|&&v| v >= 247).count();
                if snap {
                    let rule = f32_to_u32_clamped(scale.round()) as usize;
                    assert_eq!(soft, 0, "soft rows at {scale}: {column:?}");
                    assert_eq!(
                        lit,
                        rule + 1,
                        "the rule and the hairline at {scale}: {column:?}"
                    );
                }
                if snap {
                    let border = f32_to_u32_clamped(scale.round()) as usize;
                    let row = f32_to_u32_clamped(20.0 * scale);
                    let line: Vec<u32> = (0..w)
                        .map(|x| pixel_at(r.pixels(), w, x, row) & 0xFF)
                        .collect();
                    let soft = line.iter().filter(|&&v| v > 8 && v < 247).count();
                    assert_eq!(soft, 0, "{line:?}");
                    let lit = line.iter().filter(|&&v| v >= 247).count();
                    assert_eq!(lit, 2 * border, "{line:?}");
                }
            }
        }
    }

    #[test]
    fn image_dest_rects_land_on_their_physical_pixels() {
        let green = Color::rgb(0, 255, 0);
        let image = Image::new(4, 4, [0, 255, 0, 255].repeat(16));
        for scale in [1.25f32, 1.5, 2.0] {
            for zoomed in [false, true] {
                let r = frame_at(scale, |ctx| {
                    ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                    ctx.set_sampling(Sampling::Nearest).unwrap();
                    if zoomed {
                        ctx.save().unwrap();
                        ctx.transform(
                            Transform::scale(5.0, 5.0).multiply(Transform::translate(10.0, 10.0)),
                        )
                        .unwrap();
                        ctx.draw_image_rect(&image, Rect::new(0.0, 0.0, 4.0, 4.0))
                            .unwrap();
                        ctx.restore().unwrap();
                    } else {
                        ctx.draw_image_rect(&image, Rect::new(10.0, 10.0, 20.0, 20.0))
                            .unwrap();
                    }
                });
                let (_, _, w, _) = r.get_buffer();
                let at = |logical: f32| f32_to_u32_clamped((logical * scale).floor());
                let px = |x: f32, y: f32| pixel_at(r.pixels(), w, at(x), at(y));
                let case = format!("at {scale}, zoomed {zoomed}");
                assert_eq!(px(10.5, 10.5), color_to_u32(green), "inside {case}");
                assert_eq!(px(29.5, 29.5), color_to_u32(green), "far corner {case}");
                assert_ne!(px(9.0, 9.0), color_to_u32(green), "outside {case}");
                assert_ne!(px(31.0, 31.0), color_to_u32(green), "past it {case}");
            }
        }
    }
}

#[cfg(test)]
mod stroke_circle_tests {
    use super::*;
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    const SIDE: u32 = 700;

    /// The stroke loop as it was before the row-span rewrite, with the same bounding box.
    #[allow(clippy::too_many_arguments)]
    fn reference_draw_circle(
        center: Point,
        radius: f32,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let x0 = f32_to_u32_clamped((center.x - radius).floor().max(clip.left())).min(clip.x1);
        let y0 = f32_to_u32_clamped((center.y - radius).floor().max(clip.top())).min(clip.y1);
        let x1 = f32_to_u32_clamped((center.x + radius).ceil().max(clip.left())).min(clip.x1);
        let y1 = f32_to_u32_clamped((center.y + radius).ceil().max(clip.top())).min(clip.y1);
        let inner_r = (radius - paint.stroke_width).max(0.0);
        let c = color_to_u32(paint.color);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - center.x;
                let dy = y as f32 + 0.5 - center.y;
                let d = (dx * dx + dy * dy).sqrt();
                if d <= radius && d >= inner_r {
                    CpuRasterizer::buf_set(buf, bw, clip, x as i32, y as i32, c, mode);
                }
            }
        }
    }

    /// A non-uniform backdrop with varied alpha, so blends and overwrites are both visible.
    fn backdrop() -> Vec<u32> {
        let mut state = 0x2545_f491_u32;
        (0..SIDE * SIDE)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state
            })
            .collect()
    }

    fn assert_matches_reference(
        center: Point,
        radius: f32,
        paint: &Paint,
        mode: BlendMode,
        clip: ClipBox,
    ) {
        let mut expected = backdrop();
        let mut actual = expected.clone();
        reference_draw_circle(center, radius, paint, mode, &mut expected, SIDE, clip);
        CpuRasterizer::draw_circle(center, radius, paint, mode, &mut actual, SIDE, clip);
        assert!(
            actual == expected,
            "r={radius} w={} center=({}, {}) clip={clip:?} mode={mode:?} color={:?}",
            paint.stroke_width,
            center.x,
            center.y,
            paint.color,
        );
    }

    fn check_ring(radius: f32, width: f32) {
        let centers = [
            Point::new(350.0, 350.0),
            Point::new(350.37, 349.61),
            Point::new(-40.2, 120.7),
            Point::new(620.5, 640.25),
        ];
        let clips = [
            ClipBox::surface(SIDE, SIDE),
            ClipBox {
                x0: 90,
                y0: 70,
                x1: 510,
                y1: 600,
            },
        ];
        let colors = [
            Color::rgba(200, 40, 90, 255),
            Color::rgba(30, 220, 120, 128),
        ];
        for center in centers {
            for clip in clips {
                for color in colors {
                    for mode in [BlendMode::Normal, BlendMode::Multiply] {
                        let paint = Paint::new()
                            .color(color)
                            .style(PaintStyle::Stroke)
                            .stroke_width(width);
                        assert_matches_reference(center, radius, &paint, mode, clip);
                    }
                }
            }
        }
    }

    #[test]
    fn stroked_circles_match_reference_exactly() {
        let radii = [0.5_f32, 1.0, 3.3, 10.0, 57.5, 300.0];
        for radius in radii {
            let widths = [0.5_f32, 1.0, 2.5, 12.0, radius * 1.5 + 1.0];
            for width in widths {
                check_ring(radius, width);
            }
        }
    }

    #[test]
    #[ignore = "timing; run with --include-ignored --nocapture"]
    fn timing_200_stroked_circles_r300_w3() {
        const SIDE_T: u32 = 1024;
        let clip = ClipBox::surface(SIDE_T, SIDE_T);
        let paint = Paint::new()
            .color(Color::rgba(200, 40, 90, 255))
            .style(PaintStyle::Stroke)
            .stroke_width(3.0);
        let centers: Vec<Point> = (0..200)
            .map(|i| {
                let t = i as f32;
                Point::new(150.0 + (t * 37.3) % 724.0, 150.0 + (t * 91.7) % 724.0)
            })
            .collect();
        let mut best = Duration::MAX;
        for _ in 0..3 {
            let mut buf = vec![0u32; (SIDE_T * SIDE_T) as usize];
            let start = Instant::now();
            for &center in &centers {
                CpuRasterizer::draw_circle(
                    center,
                    300.0,
                    &paint,
                    BlendMode::Normal,
                    &mut buf,
                    SIDE_T,
                    clip,
                );
            }
            best = best.min(start.elapsed());
            black_box(&buf);
        }
        println!("best of 3 run: {best:?}");
    }
}

#[cfg(test)]
mod gradient_and_scaled_tests {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    const W: u32 = 40;
    const H: u32 = 30;

    /// `fill_linear_gradient` as it stands before the axis-aligned paths, kept as the reference.
    fn reference_linear(
        grad: &LinearGradient,
        rect: Rect,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let (start, end) = grad.pixel_axis();
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq < 1e-10 {
            return;
        }
        let lut = CpuRasterizer::build_gradient_lut(&grad.stops);
        let x0 = f32_to_i32_clamped(rect.x.max(clip.left()).ceil());
        let y0 = f32_to_i32_clamped(rect.y.max(clip.top()).ceil());
        let x1 = f32_to_i32_clamped((rect.x + rect.width).min(clip.right()).floor());
        let y1 = f32_to_i32_clamped((rect.y + rect.height).min(clip.bottom()).floor());
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let dt_x = dx / len_sq;
        let luts = LinearLuts::get();
        let steps = (mode == BlendMode::Normal).then(|| gradient_steps(&lut, luts));

        for cy in y0..y1 {
            let row = (cy.unsigned_abs() * bw) as usize;
            let mut t =
                ((x0 as f32 + 0.5 - start.x) * dx + (cy as f32 + 0.5 - start.y) * dy) / len_sq;
            for cx in x0..x1 {
                let t_idx = f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round());
                let idx = row + cx.unsigned_abs() as usize;
                buf[idx] = match &steps {
                    Some(steps) => gradient_over(&steps[t_idx], buf[idx], luts),
                    None => blend_pixel(lut[t_idx], buf[idx], mode),
                };
                t += dt_x;
            }
        }
    }

    /// `fill_radial_gradient` as it stands before the squared-distance path, kept as the reference.
    fn reference_radial(
        grad: &RadialGradient,
        rect: Rect,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        if grad.radius <= 0.0 {
            return;
        }
        let lut = CpuRasterizer::build_gradient_lut(&grad.stops);
        let x0 = f32_to_i32_clamped(rect.x.max(clip.left()).ceil());
        let y0 = f32_to_i32_clamped(rect.y.max(clip.top()).ceil());
        let x1 = f32_to_i32_clamped((rect.x + rect.width).min(clip.right()).floor());
        let y1 = f32_to_i32_clamped((rect.y + rect.height).min(clip.bottom()).floor());
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let inv_radius = 1.0 / grad.radius;
        let luts = LinearLuts::get();
        let steps = (mode == BlendMode::Normal).then(|| gradient_steps(&lut, luts));

        for cy in y0..y1 {
            let row = (cy.unsigned_abs() * bw) as usize;
            let dy = cy as f32 + 0.5 - grad.center.y;
            let dy_sq = dy * dy;
            for cx in x0..x1 {
                let dx = cx as f32 + 0.5 - grad.center.x;
                let dist = (dx * dx + dy_sq).sqrt();
                let t = (dist * inv_radius).min(1.0);
                let t_idx = f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round());
                let idx = row + cx.unsigned_abs() as usize;
                buf[idx] = match &steps {
                    Some(steps) => gradient_over(&steps[t_idx], buf[idx], luts),
                    None => blend_pixel(lut[t_idx], buf[idx], mode),
                };
            }
        }
    }

    /// The nearest-sampled scaled image loop, with the span arithmetic `draw_image` uses.
    #[allow(clippy::too_many_arguments)]
    fn reference_scaled(
        image: &Image,
        src: Rect,
        dest: Rect,
        mode: BlendMode,
        alpha: f32,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let centre_span = |from: f32, to: f32, lo: u32, hi: u32| {
            (
                f32_to_i32_clamped((from - 0.5).ceil()).max(lo as i32),
                f32_to_i32_clamped((to - 0.5).ceil()).min(hi as i32),
            )
        };
        let (x0, x1) = centre_span(dest.x, dest.x + dest.width, clip.x0, clip.x1);
        let (y0, y1) = centre_span(dest.y, dest.y + dest.height, clip.y0, clip.y1);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let max_sx = image.width as f32 - 0.001;
        let max_sy = image.height as f32 - 0.001;
        for cy in y0..y1 {
            let v = (cy as f32 + 0.5 - dest.y) / dest.height * src.height + src.y;
            let sy = f32_to_u32_clamped(v.clamp(0.0, max_sy));
            let src_row = &image.data[sy as usize * image.width as usize * 4..];
            for cx in x0..x1 {
                let u = (cx as f32 + 0.5 - dest.x) / dest.width * src.width + src.x;
                let sx = f32_to_u32_clamped(u.clamp(0.0, max_sx));
                let ii = sx as usize * 4;
                if ii + 3 >= src_row.len() {
                    continue;
                }
                let rgba = (u32::from(scale_u8(src_row[ii + 3], alpha)) << 24)
                    | (u32::from(src_row[ii]) << 16)
                    | (u32::from(src_row[ii + 1]) << 8)
                    | u32::from(src_row[ii + 2]);
                CpuRasterizer::buf_set(buf, bw, clip, cx, cy, rgba, mode);
            }
        }
    }

    /// Mixed opaque, translucent, transparent and empty destination pixels.
    fn background(len: u32) -> Vec<u32> {
        (0..len)
            .map(|i| {
                let h = i.wrapping_mul(2_654_435_761);
                match i % 5 {
                    0 => 0,
                    1 | 3 => 0xff00_0000 | (h & 0x00ff_ffff),
                    _ => (h & 0xff00_0000) | (h & 0x00ff_ffff),
                }
            })
            .collect()
    }

    fn stop(offset: f32, r: u8, g: u8, b: u8, a: u8) -> GradientStop {
        GradientStop {
            offset,
            color: Color::rgba(r, g, b, a),
        }
    }

    fn stop_sets() -> Vec<Vec<GradientStop>> {
        vec![
            vec![stop(0.0, 255, 255, 255, 255), stop(1.0, 255, 255, 255, 0)],
            vec![stop(0.0, 0, 0, 0, 0), stop(1.0, 0, 0, 0, 255)],
            vec![stop(0.0, 255, 0, 0, 255), stop(1.0, 0, 0, 255, 255)],
            vec![
                stop(0.0, 200, 30, 10, 90),
                stop(0.4, 10, 200, 30, 200),
                stop(1.0, 30, 40, 250, 0),
            ],
            vec![stop(0.2, 90, 90, 90, 128), stop(0.8, 20, 20, 20, 64)],
        ]
    }

    fn clips() -> Vec<ClipBox> {
        vec![
            ClipBox::surface(W, H),
            ClipBox {
                x0: 3,
                y0: 2,
                x1: 33,
                y1: 25,
            },
            ClipBox::surface(W, H).intersect(Rect::new(2.4, 1.6, 17.2, 14.9)),
        ]
    }

    fn assert_same(got: &[u32], want: &[u32], case: &str) {
        for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
            assert_eq!(
                g,
                w,
                "{case}: pixel ({}, {})",
                i % W as usize,
                i / W as usize
            );
        }
    }

    /// Within one level per channel, for the integer gradient mix.
    fn assert_close(got: &[u32], want: &[u32], case: &str) {
        for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
            let off = [0, 8, 16, 24].map(|s| ((g >> s) & 255).abs_diff((w >> s) & 255));
            assert!(
                off.iter().all(|&d| d <= 1),
                "{case}: pixel ({}, {}) {g:08x} vs {w:08x}",
                i % W as usize,
                i / W as usize
            );
        }
    }

    #[test]
    fn linear_gradients_match_the_reference() {
        let rects = [
            Rect::new(0.0, 0.0, W as f32, H as f32),
            Rect::new(1.5, 2.25, 33.5, 20.5),
            Rect::new(4.0, 1.0, 20.0, 25.0),
        ];
        let axes = [
            (Point::new(2.0, 0.0), Point::new(30.0, 0.0)),
            (Point::new(0.0, 0.0), Point::new(0.0, H as f32)),
            (Point::new(5.0, 3.0), Point::new(25.0, 22.0)),
        ];
        for stops in stop_sets() {
            for (start, end) in axes {
                let grad = LinearGradient {
                    start,
                    end,
                    stops: stops.clone(),
                };
                for rect in rects {
                    for clip in clips() {
                        for mode in [BlendMode::Normal, BlendMode::Multiply] {
                            let mut want = background(W * H);
                            reference_linear(&grad, rect, mode, &mut want, W, clip);
                            let mut got = background(W * H);
                            CpuRasterizer::fill_linear_gradient(
                                &grad, rect, mode, &mut got, W, clip,
                            );
                            assert_same(
                                &got,
                                &want,
                                &format!("{mode:?} {start:?}->{end:?} {rect:?}"),
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn radial_gradients_match_the_reference() {
        let rects = [
            Rect::new(0.0, 0.0, W as f32, H as f32),
            Rect::new(3.0, 2.0, 30.0, 25.0),
        ];
        for stops in stop_sets() {
            let grad = RadialGradient {
                center: Point::new(14.5, 12.25),
                radius: 9.0,
                stops,
            };
            for rect in rects {
                for clip in clips() {
                    for mode in [BlendMode::Normal, BlendMode::Multiply] {
                        let mut want = background(W * H);
                        reference_radial(&grad, rect, mode, &mut want, W, clip);
                        let mut got = background(W * H);
                        CpuRasterizer::fill_radial_gradient(&grad, rect, mode, &mut got, W, clip);
                        assert_close(&got, &want, &format!("{mode:?} radial {rect:?}"));
                    }
                }
            }
        }
    }

    fn image_pattern(w: u32, h: u32, alpha: impl Fn(u32, u32) -> u8) -> Image {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = ((x * 37) ^ (y * 91)).to_le_bytes()[0];
                data.extend([v, v.wrapping_mul(3), 255 - v, alpha(x, y)]);
            }
        }
        Image::new(w, h, data)
    }

    #[test]
    fn scaled_images_match_the_reference() {
        let images = [
            image_pattern(7, 5, |_, _| 255),
            image_pattern(7, 5, |x, y| [255, 0, 128, 255, 7][((x + y) % 5) as usize]),
            image_pattern(7, 5, |_, _| 128),
        ];
        // Up, down, partly offscreen, fractional, and a sub-rectangle of the source.
        let cases = [
            (
                Rect::new(0.0, 0.0, 7.0, 5.0),
                Rect::new(-3.0, 4.5, 60.0, 40.0),
            ),
            (Rect::new(0.0, 0.0, 7.0, 5.0), Rect::new(2.0, 1.0, 3.0, 2.0)),
            (
                Rect::new(0.0, 0.0, 7.0, 5.0),
                Rect::new(1.25, 3.5, 22.5, 17.75),
            ),
            (
                Rect::new(1.0, 1.0, 5.0, 3.0),
                Rect::new(-5.0, -2.0, 50.0, 31.0),
            ),
        ];
        for image in &images {
            for (src, dest) in cases {
                for clip in clips() {
                    for mode in [BlendMode::Normal, BlendMode::Multiply] {
                        for alpha in [1.0, 0.5] {
                            let mut want = background(W * H);
                            reference_scaled(image, src, dest, mode, alpha, &mut want, W, clip);
                            let mut got = background(W * H);
                            CpuRasterizer::draw_image(
                                image,
                                src,
                                dest,
                                Sampling::Nearest,
                                mode,
                                alpha,
                                &mut got,
                                &mut ImageScratch::default(),
                                W,
                                clip,
                            );
                            assert_same(&got, &want, &format!("{mode:?} {alpha} {dest:?}"));
                        }
                    }
                }
            }
        }
    }

    fn linear_bench(label: &str, grad: &LinearGradient) {
        const BW: u32 = 627;
        const BH: u32 = 202;
        const ITERS: u32 = 50;
        let rect = Rect::new(0.0, 0.0, BW as f32, BH as f32);
        let clip = ClipBox::surface(BW, BH);
        let mut buf = background(BW * BH);
        let start = Instant::now();
        for _ in 0..ITERS {
            CpuRasterizer::fill_linear_gradient(
                black_box(grad),
                rect,
                BlendMode::Normal,
                black_box(&mut buf),
                BW,
                clip,
            );
        }
        let ns = start.elapsed().as_secs_f64() * 1e9 / f64::from(ITERS * BW * BH);
        eprintln!("{label}: {ns:.2} ns/pixel");
        black_box(buf);
    }

    #[test]
    #[ignore = "manual translucent gradient timing"]
    fn translucent_gradient_benchmark() {
        let mut sets = stop_sets();
        let clear_to_black = LinearGradient {
            start: Point::new(0.0, 0.0),
            end: Point::new(627.0, 0.0),
            stops: sets.remove(1),
        };
        let white_to_clear = LinearGradient {
            start: Point::new(0.0, 0.0),
            end: Point::new(627.0, 0.0),
            stops: sets.remove(0),
        };
        linear_bench("white->clear 627x202", &white_to_clear);
        linear_bench("clear->black 627x202", &clear_to_black);
    }

    #[test]
    #[ignore = "manual radial gradient timing"]
    fn translucent_radial_benchmark() {
        let grad = RadialGradient {
            center: Point::new(313.0, 101.0),
            radius: 320.0,
            stops: stop_sets().remove(0),
        };
        let (w, h) = (627u32, 202u32);
        let rect = Rect::new(0.0, 0.0, w as f32, h as f32);
        let clip = ClipBox::surface(w, h);
        let mut buf = background(w * h);
        let iters = 50;
        let start = Instant::now();
        for _ in 0..iters {
            CpuRasterizer::fill_radial_gradient(
                black_box(&grad),
                rect,
                BlendMode::Normal,
                black_box(&mut buf),
                w,
                clip,
            );
        }
        let ns = start.elapsed().as_secs_f64() * 1e9 / f64::from(iters * w * h);
        eprintln!("radial white->clear 627x202: {ns:.2} ns/pixel");
        black_box(buf);
    }

    fn white_to_clear() -> Vec<GradientStop> {
        stop_sets().remove(0)
    }

    fn timing(label: &str, w: u32, h: u32, draw: impl Fn(&mut [u32], u32, ClipBox)) {
        const ITERS: u32 = 50;
        let clip = ClipBox::surface(w, h);
        let mut buf = background(w * h);
        let start = Instant::now();
        for _ in 0..ITERS {
            draw(black_box(&mut buf), w, clip);
        }
        let ns = start.elapsed().as_secs_f64() * 1e9 / f64::from(ITERS * w * h);
        eprintln!("{label}: {ns:.2} ns/pixel");
        black_box(buf);
    }

    #[test]
    #[ignore = "manual horizontal white-to-clear gradient timing"]
    fn timing_white_to_clear_horizontal() {
        let grad = LinearGradient {
            start: Point::new(0.0, 0.0),
            end: Point::new(627.0, 0.0),
            stops: white_to_clear(),
        };
        let rect = Rect::new(0.0, 0.0, 627.0, 202.0);
        timing(
            "horizontal white->clear 627x202",
            627,
            202,
            |buf, w, clip| {
                CpuRasterizer::fill_linear_gradient(
                    black_box(&grad),
                    rect,
                    BlendMode::Normal,
                    buf,
                    w,
                    clip,
                );
            },
        );
    }

    #[test]
    #[ignore = "manual vertical white-to-clear gradient timing"]
    fn timing_white_to_clear_vertical() {
        let grad = LinearGradient {
            start: Point::new(0.0, 0.0),
            end: Point::new(0.0, 202.0),
            stops: white_to_clear(),
        };
        let rect = Rect::new(0.0, 0.0, 627.0, 202.0);
        timing("vertical white->clear 627x202", 627, 202, |buf, w, clip| {
            CpuRasterizer::fill_linear_gradient(
                black_box(&grad),
                rect,
                BlendMode::Normal,
                buf,
                w,
                clip,
            );
        });
    }

    #[test]
    #[ignore = "manual radial white-to-clear gradient timing"]
    fn timing_radial_white_to_clear_600() {
        let grad = RadialGradient {
            center: Point::new(300.0, 300.0),
            radius: 300.0,
            stops: white_to_clear(),
        };
        let rect = Rect::new(0.0, 0.0, 600.0, 600.0);
        timing("radial white->clear 600x600", 600, 600, |buf, w, clip| {
            CpuRasterizer::fill_radial_gradient(
                black_box(&grad),
                rect,
                BlendMode::Normal,
                buf,
                w,
                clip,
            );
        });
    }

    #[test]
    #[ignore = "manual nearest image scaling timing"]
    fn nearest_scaled_image_benchmark() {
        let (w, h) = (627u32, 202u32);
        let opaque = image_pattern(320, 200, |_, _| 255);
        let translucent = image_pattern(320, 200, |_, _| 128);
        let rect = Rect::new(0.0, 0.0, w as f32, h as f32);
        let clip = ClipBox::surface(w, h);
        let iters = 50;
        for (label, image) in [("opaque", &opaque), ("translucent", &translucent)] {
            let mut buf = vec![0xff20_4060; (w * h) as usize];
            let start = Instant::now();
            for _ in 0..iters {
                CpuRasterizer::draw_image(
                    black_box(image),
                    Rect::new(0.0, 0.0, 320.0, 200.0),
                    rect,
                    Sampling::Nearest,
                    BlendMode::Normal,
                    1.0,
                    black_box(&mut buf),
                    &mut ImageScratch::default(),
                    w,
                    clip,
                );
            }
            let ns = start.elapsed().as_secs_f64() * 1e9 / f64::from(iters * w * h);
            eprintln!("nearest scaled {label} 320x200 -> 627x202: {ns:.2} ns/pixel");
            black_box(buf);
        }
    }
}

#[cfg(test)]
mod rect_span_tests {
    //! Oracles are copies of the pre-change code. The tests compare whole buffers against them.

    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    #[allow(clippy::too_many_arguments)]
    fn ref_draw_rect(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        let reach = match paint.style {
            PaintStyle::Fill => 0.0,
            PaintStyle::Stroke => paint.stroke_width / 2.0,
        };
        let x0 = f32_to_u32_clamped((rect.x - reach).floor().max(clip.left())).min(clip.x1);
        let y0 = f32_to_u32_clamped((rect.y - reach).floor().max(clip.top())).min(clip.y1);
        let x1 =
            f32_to_u32_clamped((rect.x + rect.width + reach).ceil().max(clip.left())).min(clip.x1);
        let y1 =
            f32_to_u32_clamped((rect.y + rect.height + reach).ceil().max(clip.top())).min(clip.y1);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        match paint.style {
            PaintStyle::Fill => {
                ref_fill_rect_region(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1)
            }
            PaintStyle::Stroke => {
                ref_stroke_rect_region(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ref_fill_rect_region(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        if paint.color.a == 255 && mode == BlendMode::Normal && on_pixel_grid(rect) {
            let c = color_to_u32(paint.color);
            for y in y0..y1 {
                let start = (y * bw + x0) as usize;
                let end = (y * bw + x1) as usize;
                if end <= buf.len() {
                    buf[start..end].fill(c);
                }
            }
        } else {
            ref_fill_rect_region_translucent(rect, paint, mode, buf, bw, clip, x0, y0, x1, y1);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ref_fill_rect_region_translucent(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        let xl = rect.x;
        let xr = rect.x + rect.width;
        let yl = rect.y;
        let yr = rect.y + rect.height;

        let xi0 = f32_to_i32_clamped(xl.ceil())
            .clamp(x0 as i32, x1 as i32)
            .unsigned_abs();
        let xi1 = f32_to_i32_clamped(xr.floor())
            .clamp(x0 as i32, x1 as i32)
            .unsigned_abs();
        let has_full_x = xi0 < xi1;
        let yi0 = f32_to_i32_clamped(yl.ceil())
            .clamp(y0 as i32, y1 as i32)
            .unsigned_abs();
        let yi1 = f32_to_i32_clamped(yr.floor())
            .clamp(y0 as i32, y1 as i32)
            .unsigned_abs();

        let const_src =
            (mode == BlendMode::Normal).then(|| ConstSrc::new(color_to_u32(paint.color)));
        let ctx = RectFillCtx {
            clip,
            paint,
            mode,
            xl,
            xr,
            c_full: color_to_u32(paint.color),
            opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
            const_src,
        };

        for y in y0..y1 {
            let row_cov = if y >= yi0 && y < yi1 {
                1.0
            } else {
                rect_cov_y(y, yl, yr)
            };
            if row_cov <= 0.0 {
                continue;
            }
            ref_fill_rect_row_translucent(buf, bw, &ctx, x0, x1, xi0, xi1, has_full_x, y, row_cov);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ref_fill_rect_row_translucent(
        buf: &mut [u32],
        bw: u32,
        ctx: &RectFillCtx,
        x0: u32,
        x1: u32,
        xi0: u32,
        xi1: u32,
        has_full_x: bool,
        y: u32,
        row_cov: f32,
    ) {
        if !has_full_x {
            CpuRasterizer::fill_rect_edge_span(buf, bw, ctx, x0, x1, y, row_cov);
            return;
        }

        CpuRasterizer::fill_rect_edge_span(buf, bw, ctx, x0, xi0, y, row_cov);

        if row_cov >= 1.0 {
            if ctx.opaque_fast {
                let row_start = (y * bw) as usize;
                buf[row_start + xi0 as usize..row_start + xi1 as usize].fill(ctx.c_full);
            } else if let Some(src) = ctx.const_src {
                let row_start = (y * bw) as usize;
                let span = row_start + xi0 as usize..row_start + xi1 as usize;
                if let Some(pixels) = buf.get_mut(span) {
                    src.over_span(pixels);
                }
            } else {
                for x in xi0..xi1 {
                    CpuRasterizer::buf_set(
                        buf, bw, ctx.clip, x as i32, y as i32, ctx.c_full, ctx.mode,
                    );
                }
            }
        } else {
            for x in xi0..xi1 {
                CpuRasterizer::blend_at(buf, bw, ctx, x, y, row_cov);
            }
        }

        CpuRasterizer::fill_rect_edge_span(buf, bw, ctx, xi1, x1, y, row_cov);
    }

    #[allow(clippy::too_many_arguments)]
    fn ref_stroke_rect_region(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        let half = paint.stroke_width / 2.0;
        if half <= 0.0 {
            return;
        }
        let outer = Rect::new(
            rect.x - half,
            rect.y - half,
            rect.width + 2.0 * half,
            rect.height + 2.0 * half,
        );
        let inner = Rect::new(
            rect.x + half,
            rect.y + half,
            (rect.width - 2.0 * half).max(0.0),
            (rect.height - 2.0 * half).max(0.0),
        );
        if inner.width <= 0.0 || inner.height <= 0.0 {
            let fill = paint.clone().style(PaintStyle::Fill);
            return ref_fill_rect_region(&outer, &fill, mode, buf, bw, clip, x0, y0, x1, y1);
        }
        let const_src =
            (mode == BlendMode::Normal).then(|| ConstSrc::new(color_to_u32(paint.color)));
        let ctx = RectFillCtx {
            clip,
            paint,
            mode,
            xl: outer.x,
            xr: outer.right(),
            c_full: color_to_u32(paint.color),
            opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
            const_src,
        };
        let band = |from: f32, to: f32| {
            f32_to_u32_clamped(from.floor()).clamp(x0, x1)
                ..f32_to_u32_clamped(to.ceil()).clamp(x0, x1)
        };
        let left = band(outer.x, inner.x);
        let right = band(inner.right(), outer.right());
        for y in y0..y1 {
            let out_y = rect_cov_y(y, outer.y, outer.bottom());
            if out_y <= 0.0 {
                continue;
            }
            let in_y = rect_cov_y(y, inner.y, inner.bottom());
            let mut stroke_at = |x: u32| {
                let cov = out_y * rect_cov_x(x, outer.x, outer.right())
                    - in_y * rect_cov_x(x, inner.x, inner.right());
                if cov <= 0.0 {
                    return;
                }
                if cov >= 1.0 && ctx.opaque_fast {
                    CpuRasterizer::buf_set(buf, bw, clip, x as i32, y as i32, ctx.c_full, mode);
                } else {
                    CpuRasterizer::blend_at(buf, bw, &ctx, x, y, cov.min(1.0));
                }
            };
            if in_y >= 1.0 {
                left.clone().for_each(&mut stroke_at);
                right.clone().for_each(&mut stroke_at);
            } else {
                (x0..x1).for_each(&mut stroke_at);
            }
        }
    }

    /// Pre-change `end_frame`, kept as the oracle for the clear pass.
    impl CpuRasterizer {
        fn reference_end_frame(&mut self) -> AureaResult<FrameOutcome> {
            let (bw, bh) = (self.width, self.height);
            let pending = self.pending_damage.take();
            let diff = self.diff_damage();

            let Some(damage) = resolve_frame_damage(pending, diff, bw, bh) else {
                return Ok(FrameOutcome::Unchanged);
            };
            if damage.is_some_and(|r| {
                r.width <= 0.0
                    || r.height <= 0.0
                    || r.x >= bw as f32
                    || r.y >= bh as f32
                    || r.x + r.width <= 0.0
                    || r.y + r.height <= 0.0
            }) {
                self.last_frame_damage = None;
                self.capture_prev_items();
                return Ok(FrameOutcome::Unchanged);
            }

            let (tiles_x, tiles_y) = tile_grid_dims(bw, bh);
            let dirty_tiles = self.compute_dirty_tiles(damage, pending, tiles_x, tiles_y);

            self.last_frame_damage = union_dirty_tile_rects(&dirty_tiles, tiles_x, tiles_y, bw, bh);
            if self.last_frame_damage.is_none() {
                self.scratch_dirty = dirty_tiles;
                self.capture_prev_items();
                return Ok(FrameOutcome::Unchanged);
            }

            let mut dirty_tiles = dirty_tiles;
            if let Some(region) = self.last_frame_damage {
                mark_tile_range_dirty(region, tiles_x, tiles_y, &mut dirty_tiles);
            }

            let surface_clip = match self.last_frame_damage {
                Some(region) => ClipBox::surface(bw, bh).intersect(region),
                None => ClipBox::surface(bw, bh),
            };
            let items = self.display_list.items();
            let mut occluded = mem::take(&mut self.scratch_occluded);
            let last_clear = mark_occluded(
                items,
                (tiles_x, tiles_y, bw, bh),
                &mut self.scratch_covered,
                &mut occluded,
                &mut self.scratch_under_clear,
            );
            for (i, item) in items.iter().enumerate() {
                if let DrawCommand::Clear(color) = &item.command {
                    let (color, clip) =
                        (color_to_u32(*color), surface_clip.intersect_opt(item.clip));
                    let buf = &mut self.frame_buffer;
                    if last_clear == Some(i) {
                        let dims = (tiles_x, tiles_y, bw, bh);
                        uncovered_runs(&self.scratch_under_clear, dims, |run| {
                            CpuRasterizer::clear_rect(&run, color, buf, bw, clip);
                        });
                    } else {
                        let whole = Rect::new(0.0, 0.0, bw as f32, bh as f32);
                        CpuRasterizer::clear_rect(&whole, color, buf, bw, clip);
                    }
                    continue;
                }

                if !should_render_item(item, &occluded, i, &dirty_tiles, tiles_x, tiles_y) {
                    continue;
                }
                CpuRasterizer::render_clipped_item(
                    item,
                    &mut self.frame_buffer,
                    &mut self.scratch_path,
                    &mut self.scratch_row,
                    bw,
                    surface_clip.intersect_opt(item.clip),
                )?;
            }

            self.scratch_occluded = occluded;
            self.scratch_dirty = dirty_tiles;
            self.capture_prev_items();

            self.has_frame = true;
            Ok(FrameOutcome::Presented)
        }
    }

    fn dst_uniform(w: u32, h: u32) -> Vec<u32> {
        vec![0xff10_2030; (w * h) as usize]
    }

    /// Varied colours and alphas, including fully transparent pixels.
    fn dst_varied(w: u32, h: u32) -> Vec<u32> {
        (0..w * h)
            .map(|i| {
                if i % 5 == 0 {
                    0
                } else {
                    i.wrapping_mul(2_654_435_761)
                }
            })
            .collect()
    }

    const FRACTIONAL_RECTS: [(f32, f32, f32, f32); 8] = [
        (10.3, 5.6, 40.25, 20.4),
        (3.0, 2.5, 30.0, 7.75),
        (-4.5, 1.25, 20.5, 30.5),
        (50.2, 40.7, 30.0, 20.0),
        (0.0, 0.0, 64.0, 48.0),
        (12.5, 0.4, 0.5, 30.0),
        (-100.0, -100.0, 10.0, 10.0),
        (7.9, 3.33, 45.6, 0.9),
    ];

    fn colours() -> [Color; 4] {
        [
            Color::rgba(40, 120, 200, 128),
            Color::rgba(200, 30, 90, 255),
            Color::rgba(10, 10, 10, 90),
            Color::rgb(255, 255, 255),
        ]
    }

    fn clips() -> [ClipBox; 3] {
        [
            ClipBox::surface(64, 48),
            ClipBox {
                x0: 5,
                y0: 4,
                x1: 50,
                y1: 40,
            },
            ClipBox {
                x0: 0,
                y0: 0,
                x1: 20,
                y1: 48,
            },
        ]
    }

    #[test]
    fn fill_rect_matches_reference_over_fractional_rects() {
        const W: u32 = 64;
        const H: u32 = 48;
        let mut cases = 0;
        for (x, y, w, h) in FRACTIONAL_RECTS {
            let rect = Rect::new(x, y, w, h);
            for color in colours() {
                let paint = Paint::new().color(color);
                for mode in [BlendMode::Normal, BlendMode::Multiply] {
                    for clip in clips() {
                        for dst in [dst_uniform(W, H), dst_varied(W, H)] {
                            let mut new = dst.clone();
                            let mut old = dst;
                            CpuRasterizer::draw_rect(&rect, &paint, mode, &mut new, W, clip);
                            ref_draw_rect(&rect, &paint, mode, &mut old, W, clip);
                            assert_eq!(
                                new, old,
                                "rect {rect:?} colour {color:?} mode {mode:?} clip {clip:?}"
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 8 * 4 * 2 * 3 * 2);
    }

    #[test]
    fn stroke_rect_matches_reference_over_fractional_rects() {
        const W: u32 = 64;
        const H: u32 = 48;
        for (x, y, w, h) in FRACTIONAL_RECTS {
            let rect = Rect::new(x, y, w, h);
            for color in colours() {
                for width in [1.0, 2.5, 6.0, 40.0] {
                    let paint = Paint::new()
                        .color(color)
                        .style(PaintStyle::Stroke)
                        .stroke_width(width);
                    for mode in [BlendMode::Normal, BlendMode::Multiply] {
                        for clip in clips() {
                            for dst in [dst_uniform(W, H), dst_varied(W, H)] {
                                let mut new = dst.clone();
                                let mut old = dst;
                                CpuRasterizer::draw_rect(&rect, &paint, mode, &mut new, W, clip);
                                ref_draw_rect(&rect, &paint, mode, &mut old, W, clip);
                                assert_eq!(
                                    new, old,
                                    "rect {rect:?} colour {color:?} width {width} mode {mode:?} clip {clip:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    type Scene = fn(&mut dyn DrawingContext, u8);

    fn tint(k: u8, base: u8) -> Color {
        Color::rgb(
            base.wrapping_add(k.wrapping_mul(30)),
            60u8.wrapping_add(k.wrapping_mul(17)),
            140u8.wrapping_sub(k.wrapping_mul(9)),
        )
    }

    fn solid(color: Color) -> Paint {
        Paint::new().color(color)
    }

    fn scene_translucent_under_clear(ctx: &mut dyn DrawingContext, k: u8) {
        ctx.clear(tint(k, 20)).unwrap();
        ctx.draw_rect(
            Rect::new(10.3, 5.6, 40.25, 20.4),
            &solid(Color::rgba(40, 120, 200, 128)),
        )
        .unwrap();
        ctx.clear(tint(k, 90)).unwrap();
        ctx.draw_rect(
            Rect::new(20.0, 10.0, 30.0, 16.0),
            &solid(Color::rgb(200, 30, 90)),
        )
        .unwrap();
    }

    fn scene_clipped_last_clear(ctx: &mut dyn DrawingContext, k: u8) {
        ctx.clear(tint(k, 20)).unwrap();
        ctx.save().unwrap();
        ctx.clip_rect(Rect::new(10.0, 10.0, 40.0, 20.0)).unwrap();
        ctx.clear(tint(k, 150)).unwrap();
        ctx.restore().unwrap();
        ctx.draw_rect(
            Rect::new(20.5, 4.25, 30.0, 12.5),
            &solid(Color::rgba(10, 10, 10, 90)),
        )
        .unwrap();
    }

    fn scene_clipped_earlier_clear(ctx: &mut dyn DrawingContext, k: u8) {
        ctx.save().unwrap();
        ctx.clip_rect(Rect::new(4.0, 4.0, 80.0, 40.0)).unwrap();
        ctx.clear(tint(k, 20)).unwrap();
        ctx.restore().unwrap();
        ctx.clear(tint(k, 150)).unwrap();
    }

    fn scene_translucent_clears(ctx: &mut dyn DrawingContext, k: u8) {
        let a = tint(k, 40);
        ctx.clear(Color::rgba(a.r, a.g, a.b, 100)).unwrap();
        ctx.clear(Color::rgba(a.b, a.r, a.g, 200)).unwrap();
        ctx.set_blend_mode(BlendMode::Multiply).unwrap();
        ctx.draw_rect(
            Rect::new(3.5, 3.5, 50.0, 30.0),
            &solid(Color::rgba(90, 200, 60, 180)),
        )
        .unwrap();
        ctx.set_blend_mode(BlendMode::Normal).unwrap();
    }

    fn scene_three_clears(ctx: &mut dyn DrawingContext, k: u8) {
        ctx.clear(tint(k, 20)).unwrap();
        ctx.clear(tint(k, 60)).unwrap();
        ctx.clear(tint(k, 100)).unwrap();
        ctx.draw_rect(
            Rect::new(7.25, 9.5, 33.5, 11.75),
            &solid(Color::rgb(250, 240, 20)),
        )
        .unwrap();
    }

    fn scene_opaque_between(ctx: &mut dyn DrawingContext, k: u8) {
        ctx.clear(tint(k, 20)).unwrap();
        ctx.draw_rect(Rect::new(0.0, 0.0, 48.0, 64.0), &solid(tint(k, 200)))
            .unwrap();
        ctx.clear(Color::rgba(0, 0, 0, 128)).unwrap();
        ctx.clip_rect(Rect::new(0.0, 0.0, 96.0, 20.0)).unwrap();
        ctx.clear(tint(k, 70)).unwrap();
    }

    fn scene_circle_then_clear(ctx: &mut dyn DrawingContext, k: u8) {
        ctx.clear(tint(k, 20)).unwrap();
        ctx.draw_circle(
            Point::new(40.0, 30.0),
            20.0,
            &solid(Color::rgb(30, 220, 200)),
        )
        .unwrap();
        ctx.clear(Color::rgba(0, 0, 0, 128)).unwrap();
    }

    fn scene_small_change(ctx: &mut dyn DrawingContext, k: u8) {
        ctx.clear(Color::rgb(30, 30, 30)).unwrap();
        ctx.clear(Color::rgb(60, 60, 60)).unwrap();
        ctx.draw_rect(
            Rect::new(30.5, 20.25, 20.0, 10.0),
            &solid(Color::rgba(k.wrapping_mul(50), 100, 200, 150)),
        )
        .unwrap();
    }

    /// Renders one frame through the old or the new `end_frame` and returns the buffer.
    fn frame(
        r: &mut CpuRasterizer,
        reference: bool,
        scene: impl FnOnce(&mut dyn DrawingContext),
    ) -> Vec<u32> {
        let mut ctx = r.begin_frame().unwrap();
        scene(&mut *ctx);
        drop(ctx);
        if reference {
            r.reference_end_frame().unwrap();
        } else {
            r.end_frame().unwrap();
        }
        r.frame_buffer.clone()
    }

    #[test]
    fn clear_pass_matches_reference_end_frame() {
        let scenes: [Scene; 9] = [
            scene_translucent_under_clear,
            scene_clipped_last_clear,
            scene_clipped_earlier_clear,
            scene_translucent_clears,
            scene_three_clears,
            scene_opaque_between,
            scene_circle_then_clear,
            scene_small_change,
            scene_translucent_under_clear,
        ];
        let ks = [0u8, 1, 2, 3, 3, 0, 5];
        for scene in scenes {
            let mut new = CpuRasterizer::new(96, 64);
            let mut old = CpuRasterizer::new(96, 64);
            for k in ks {
                let a = frame(&mut new, false, |ctx| scene(ctx, k));
                let b = frame(&mut old, true, |ctx| scene(ctx, k));
                assert!(a == b, "frame k={k} differs from the reference end_frame");
            }
        }
    }

    fn time_per_op(label: &str, iters: u32, mut op: impl FnMut()) {
        let start = Instant::now();
        for _ in 0..iters {
            op();
        }
        eprintln!(
            "{label}: {:.2} us/op",
            start.elapsed().as_secs_f64() * 1e6 / f64::from(iters)
        );
    }

    #[test]
    #[ignore = "manual fractional translucent fill timing"]
    fn fractional_translucent_fill_timing() {
        const W: u32 = 1920;
        const H: u32 = 1080;
        let mut buf = dst_uniform(W, H);
        let clip = ClipBox::surface(W, H);
        let rect = Rect::new(100.3, 50.6, 1200.4, 300.25);
        let paint = solid(Color::rgba(40, 120, 200, 128));
        time_per_op("fractional translucent fill 1200x300", 50, || {
            CpuRasterizer::draw_rect(
                black_box(&rect),
                &paint,
                BlendMode::Normal,
                black_box(&mut buf),
                W,
                clip,
            );
        });
        black_box(buf);
    }

    #[test]
    #[ignore = "manual fractional stroke timing"]
    fn fractional_stroke_timing() {
        const W: u32 = 1920;
        const H: u32 = 1080;
        let mut buf = dst_uniform(W, H);
        let clip = ClipBox::surface(W, H);
        let rect = Rect::new(100.3, 50.6, 1200.4, 300.25);
        let paint = Paint::new()
            .color(Color::rgba(40, 120, 200, 128))
            .style(PaintStyle::Stroke)
            .stroke_width(20.0);
        time_per_op("fractional translucent stroke 1200x300 w20", 50, || {
            CpuRasterizer::draw_rect(
                black_box(&rect),
                &paint,
                BlendMode::Normal,
                black_box(&mut buf),
                W,
                clip,
            );
        });
        black_box(buf);
    }

    #[test]
    #[ignore = "manual full-frame clear pass timing"]
    fn full_frame_clears_timing() {
        let mut r = CpuRasterizer::new(1920, 1080);
        let mut n = 0u8;
        time_per_op("1920x1080 frame: 4 clears + rect", 60, || {
            n = n.wrapping_add(1);
            let mut ctx = r.begin_frame().unwrap();
            ctx.clear(Color::rgb(n, 20, 20)).unwrap();
            ctx.clear(Color::rgb(n, 40, 20)).unwrap();
            ctx.clear(Color::rgb(n, 60, 20)).unwrap();
            ctx.clear(Color::rgb(n, 80, 20)).unwrap();
            ctx.draw_rect(
                Rect::new(100.0, 100.0, 300.0, 200.0),
                &solid(Color::rgb(9, 9, 9)),
            )
            .unwrap();
            drop(ctx);
            r.end_frame().unwrap();
        });
    }
}
