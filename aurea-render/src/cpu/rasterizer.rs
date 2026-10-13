//! CPU rasterizer — flat framebuffer, no tile overhead.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem;

use crate::cpu::blend::{ConstSrc, LinearLuts, blend_pixel};
use crate::cpu::clip::ClipBox;
#[cfg(test)]
use crate::cpu::damage::*;
use crate::cpu::damage::{
    FrameDamage, is_known_bounds, mark_occluded, resolve_frame_damage, should_render_item,
    union_rect,
};
use crate::cpu::raster::fill::{self, PathScratch};
use crate::cpu::raster::sample;
use crate::cpu::stroke;
#[cfg(test)]
use crate::cpu::tiles::*;
use crate::cpu::tiles::{
    mark_tile_range_dirty, tile_grid_dims, tile_range, tile_rect, union_dirty_tile_rects,
};
use crate::numeric::{
    f32_to_i32_clamped, f32_to_u8_clamped, f32_to_u32_clamped, f32_to_usize_clamped,
};
use crate::recording::RecordingContext;
use crate::renderer::{DrawingContext, FrameOutcome, FrameOutput, Renderer};
use crate::scene::command::DrawCommand;
use crate::scene::display_list::{CacheKey, DisplayItem, DisplayList};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::{
    BlendMode, Color, GlyphMask, GradientStop, Image, LinearGradient, Paint, PaintStyle, Path,
    PathCommand, Point, RadialGradient, Rect, Sampling,
};
use aurea_foundation::AureaResult;

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
    scratch_row: Vec<u32>,
    scratch_dirty: Vec<bool>,
    scratch_hashers: Vec<DefaultHasher>,
    scratch_covered: Vec<bool>,
    scratch_occluded: Vec<bool>,
    last_frame_damage: Option<Rect>,
    has_frame: bool,
}

impl CpuRasterizer {
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
            scratch_row: Vec::new(),
            scratch_dirty: Vec::new(),
            scratch_hashers: Vec::new(),
            scratch_covered: Vec::new(),
            scratch_occluded: Vec::new(),
            last_frame_damage: None,
            has_frame: false,
        }
    }

    fn raster_dimensions(lw: u32, lh: u32, scale: f32) -> (u32, u32) {
        let s = scale.max(1.0);
        (
            f32_to_u32_clamped((lw as f32 * s).round()).max(1),
            f32_to_u32_clamped((lh as f32 * s).round()).max(1),
        )
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

    pub fn display_list(&self) -> &DisplayList {
        &self.display_list
    }

    /// The size it draws at, in logical pixels.
    pub fn logical_size(&self) -> (u32, u32) {
        (self.logical_width, self.logical_height)
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
    ) -> AureaResult<Vec<u8>> {
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
        Ok(buf
            .iter()
            .flat_map(|px| {
                let a = px.to_be_bytes()[0];
                [a, a, a, a]
            })
            .collect())
    }

    pub(crate) fn render_recorded(&mut self, list: &mut DisplayList) -> AureaResult<FrameOutcome> {
        mem::swap(&mut self.display_list, list);
        let outcome = self.end_frame();
        mem::swap(&mut self.display_list, list);
        outcome
    }

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

    #[allow(clippy::too_many_arguments)]
    fn render_item(
        item: &DisplayItem,
        buf: &mut [u32],
        scratch_path: &mut PathScratch,
        scratch_row: &mut Vec<u32>,
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
                Self::draw_glyph(mask, *origin, fade_color(*color, alpha), buf, bw, clip);
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
        let x0 = f32_to_u32_clamped(rect.x.max(clip.left())).min(clip.x1);
        let y0 = f32_to_u32_clamped(rect.y.max(clip.top())).min(clip.y1);
        let x1 = f32_to_u32_clamped((rect.x + rect.width).ceil().max(clip.left())).min(clip.x1);
        let y1 = f32_to_u32_clamped((rect.y + rect.height).ceil().max(clip.top())).min(clip.y1);
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
        if paint.color.a == 255 && mode == BlendMode::Normal {
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
            } else {
                for x in xi0..xi1 {
                    Self::buf_set(buf, bw, ctx.clip, x as i32, y as i32, ctx.c_full, ctx.mode);
                }
            }
        } else {
            for x in xi0..xi1 {
                Self::blend_at(buf, bw, ctx, x, y, row_cov);
            }
        }

        Self::fill_rect_edge_span(buf, bw, ctx, xi1, x1, y, row_cov);
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
        let sw = i32::try_from(f32_to_u32_clamped(paint.stroke_width)).unwrap_or(i32::MAX);
        if sw == 0 {
            return;
        }
        let c = color_to_u32(paint.color);
        let left = f32_to_i32_clamped(rect.x.floor());
        let top = f32_to_i32_clamped(rect.y.floor());
        let right = f32_to_i32_clamped((rect.x + rect.width).ceil());
        let bottom = f32_to_i32_clamped((rect.y + rect.height).ceil());
        let sw_x = sw.min(right.saturating_sub(left));
        let sw_y = sw.min(bottom.saturating_sub(top));
        let (vx0, vx1) = (x0 as i32, x1 as i32);

        let span = |from: i32, to: i32| from.clamp(vx0, vx1)..to.clamp(vx0, vx1);
        let left_band = span(left, left.saturating_add(sw_x));
        let right_band = span(
            right.saturating_sub(sw_x).max(left.saturating_add(sw_x)),
            right,
        );

        for y in y0 as i32..y1 as i32 {
            let mut set = |x: i32| Self::buf_set(buf, bw, clip, x, y, c, mode);
            if y < top.saturating_add(sw_y) || y >= bottom.saturating_sub(sw_y) {
                (vx0..vx1).for_each(&mut set);
            } else {
                left_band.clone().for_each(&mut set);
                right_band.clone().for_each(&mut set);
            }
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
            if ctx.opaque_fast {
                let row_start = (y * bw) as usize;
                buf[row_start + xi0.unsigned_abs() as usize
                    ..row_start + xi1.unsigned_abs() as usize]
                    .fill(ctx.c_full);
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
            if cov > 0.0 {
                let c = color_to_u32_with_coverage(ctx.paint.color, cov);
                Self::buf_set(buf, bw, ctx.clip, x, y as i32, c, ctx.mode);
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
        let sw = paint.stroke_width;
        let inner_r = (radius - sw).max(0.0);
        let c = color_to_u32(paint.color);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - center.x;
                let dy = y as f32 + 0.5 - center.y;
                let d = (dx * dx + dy * dy).sqrt();
                if d <= radius && d >= inner_r {
                    Self::buf_set(buf, bw, clip, x as i32, y as i32, c, mode);
                }
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

    fn draw_glyph(
        mask: &GlyphMask,
        origin: Point,
        color: Color,
        buf: &mut [u32],
        bw: u32,
        clip: ClipBox,
    ) {
        if mask.width == 0
            || mask.height == 0
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
        let dx = f32_to_i32_clamped(origin.x.round());
        let dy = f32_to_i32_clamped(origin.y.round());

        let mw = mask.width as i32;
        let x_lo = (i32::try_from(clip.x0).unwrap_or(0) - dx).max(0);
        let x_hi = (i32::try_from(clip.x1).unwrap_or(i32::MAX) - dx).min(mw);
        if x_lo >= x_hi {
            return;
        }

        for my in 0..mask.height as i32 {
            let py = dy + my;
            if !clip.contains(dx + x_lo, py) {
                continue;
            }
            let row = (my.unsigned_abs() * mask.width) as usize;
            let cov_row = &mask.coverage[row * 3..(row + mask.width as usize) * 3];
            let buf_row = (py.unsigned_abs() * bw) as usize;

            for mx in x_lo..x_hi {
                let ci = mx.unsigned_abs() as usize * 3;
                let cr8 = cov_row[ci];
                let cg8 = cov_row[ci + 1];
                let cb8 = cov_row[ci + 2];
                if cr8 == 0 && cg8 == 0 && cb8 == 0 {
                    continue;
                }

                let idx = buf_row + (dx + mx).unsigned_abs() as usize;

                if cr8 == 255 && cg8 == 255 && cb8 == 255 {
                    buf[idx] = opaque_pixel;
                    continue;
                }

                let dst = buf[idx];
                let da = (dst >> 24) & 0xff;
                let [_, dr, dg, db] = dst.to_be_bytes();
                let or_ = luts.mix(tr, dr, cr8);
                let og = luts.mix(tg, dg, cg8);
                let ob = luts.mix(tb, db, cb8);
                let sa = u32::from(cr8.max(cg8).max(cb8));
                let oa = sa + ((255 - sa) * da) / 255;
                buf[idx] = (oa << 24) | (or_ << 16) | (og << 8) | ob;
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
        scratch_row: &mut Vec<u32>,
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
        let x0 = f32_to_i32_clamped(dest.x.max(clip.left()).ceil());
        let y0 = f32_to_i32_clamped(dest.y.max(clip.top()).ceil());
        let x1 = f32_to_i32_clamped((dest.x + dest.width).min(clip.right()).floor());
        let y1 = f32_to_i32_clamped((dest.y + dest.height).min(clip.bottom()).floor());
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
                scratch_row,
                bw,
                clip,
                x0,
                y0,
                x1,
                y1,
            );
        } else if sampling == Sampling::Nearest {
            Self::draw_image_scaled(image, src, dest, mode, alpha, buf, bw, clip, x0, y0, x1, y1);
        } else {
            Self::draw_image_filtered(
                image,
                src,
                dest,
                sampling,
                mode,
                alpha,
                buf,
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
        for cy in y0..y1 {
            let v0 = (cy as f32 - dest.y) * sy + src.y;
            for cx in x0..x1 {
                let u0 = (cx as f32 - dest.x) * sx + src.x;
                let c = if shrink {
                    sample::area(image, bounds, (u0, v0), (u0 + sx, v0 + sy), alpha)
                } else {
                    sample::bilinear(image, bounds, u0 + sx * 0.5, v0 + sy * 0.5, alpha)
                };
                if c >> 24 != 0 {
                    Self::buf_set(buf, bw, clip, cx, cy, c, mode);
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
        clip: ClipBox,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
    ) {
        let max_sx = image.width as f32 - 0.001;
        let max_sy = image.height as f32 - 0.001;
        for cy in y0..y1 {
            let v = (cy as f32 - dest.y) / dest.height * src.height + src.y;
            let sy = f32_to_u32_clamped(v.clamp(0.0, max_sy));
            let src_row = &image.data[sy as usize * image.width as usize * 4..];
            for cx in x0..x1 {
                let u = (cx as f32 - dest.x) / dest.width * src.width + src.x;
                let sx = f32_to_u32_clamped(u.clamp(0.0, max_sx));
                let ii = sx as usize * 4;
                if ii + 3 >= src_row.len() {
                    continue;
                }
                let rgba = (u32::from(scale_u8(src_row[ii + 3], alpha)) << 24)
                    | (u32::from(src_row[ii]) << 16)
                    | (u32::from(src_row[ii + 1]) << 8)
                    | u32::from(src_row[ii + 2]);
                Self::buf_set(buf, bw, clip, cx, cy, rgba, mode);
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
        let dx = grad.end.x - grad.start.x;
        let dy = grad.end.y - grad.start.y;
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
        let opaque_normal = mode == BlendMode::Normal;

        for cy in y0..y1 {
            let row = (cy.unsigned_abs() * bw) as usize;
            let mut t = ((x0 as f32 + 0.5 - grad.start.x) * dx
                + (cy as f32 + 0.5 - grad.start.y) * dy)
                / len_sq;
            for cx in x0..x1 {
                let t_idx = f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round());
                let src = lut[t_idx];
                let idx = row + cx.unsigned_abs() as usize;
                buf[idx] = if opaque_normal && (src >> 24) == 255 {
                    src
                } else {
                    blend_pixel(src, buf[idx], mode)
                };
                t += dt_x;
            }
        }
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
        let opaque_normal = mode == BlendMode::Normal;

        for cy in y0..y1 {
            let row = (cy.unsigned_abs() * bw) as usize;
            let dy = cy as f32 + 0.5 - grad.center.y;
            let dy_sq = dy * dy;
            for cx in x0..x1 {
                let dx = cx as f32 + 0.5 - grad.center.x;
                let dist = (dx * dx + dy_sq).sqrt();
                let t = (dist * inv_radius).min(1.0);
                let t_idx = f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round());
                let src = lut[t_idx];
                let idx = row + cx.unsigned_abs() as usize;
                buf[idx] = if opaque_normal && (src >> 24) == 255 {
                    src
                } else {
                    blend_pixel(src, buf[idx], mode)
                };
            }
        }
    }

    fn diff_damage(&self) -> FrameDamage {
        let new_items = self.display_list.items();
        let old_items = &self.prev_items;
        let max_len = new_items.len().max(old_items.len());
        let mut acc: Option<Rect> = None;

        for i in 0..max_len {
            let new = new_items.get(i).map(|item| (item.cache_key, item.bounds));
            let old = old_items.get(i).copied();

            let contribution = match (new, old) {
                (Some((nk, nb)), Some((ok, ob))) if nk != ok => {
                    if !is_known_bounds(nb) || !is_known_bounds(ob) {
                        return FrameDamage::Full;
                    }
                    Some(union_rect(nb, ob))
                }
                (Some((_, nb)), None) => {
                    if !is_known_bounds(nb) {
                        return FrameDamage::Full;
                    }
                    Some(nb)
                }
                (None, Some((_, ob))) => {
                    if !is_known_bounds(ob) {
                        return FrameDamage::Full;
                    }
                    Some(ob)
                }
                _ => None,
            };

            acc = match (acc, contribution) {
                (Some(a), Some(b)) => Some(union_rect(a, b)),
                (None, Some(b)) => Some(b),
                (acc, None) => acc,
            };
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
        let mut hashers = mem::take(&mut self.scratch_hashers);
        hashers.clear();
        hashers.resize(range_w * (ty1 - ty0) as usize, DefaultHasher::new());

        for item in self.display_list.items() {
            if !is_known_bounds(item.bounds) {
                continue;
            }
            let (itx0, ity0, itx1, ity1) = tile_range(item.bounds, tiles_x, tiles_y);
            let lo_x = itx0.max(tx0);
            let hi_x = itx1.min(tx1);
            let lo_y = ity0.max(ty0);
            let hi_y = ity1.min(ty1);
            for ty in lo_y..hi_y {
                for tx in lo_x..hi_x {
                    let h = &mut hashers[(ty - ty0) as usize * range_w + (tx - tx0) as usize];
                    item.cache_key.0.hash(h);
                    item.bounds.x.to_bits().hash(h);
                    item.bounds.y.to_bits().hash(h);
                    item.bounds.width.to_bits().hash(h);
                    item.bounds.height.to_bits().hash(h);
                }
            }
        }

        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                let new_hash =
                    hashers[(ty - ty0) as usize * range_w + (tx - tx0) as usize].finish();
                let idx = (ty * tiles_x + tx) as usize;
                if new_hash != self.tile_hashes[idx] {
                    dirty[idx] = true;
                }
                self.tile_hashes[idx] = new_hash;
            }
        }
        self.scratch_hashers = hashers;
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
}

impl Renderer for CpuRasterizer {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.scale_factor = info.scale_factor.max(1.0);
        self.logical_width = info.width;
        self.logical_height = info.height;
        let (rw, rh) = Self::raster_dimensions(info.width, info.height, info.scale_factor);
        self.width = rw;
        self.height = rh;
        self.frame_buffer = vec![0u32; (rw * rh) as usize];
        self.prev_items.clear();
        self.tile_hashes.clear();
        self.last_frame_damage = None;
        Ok(())
    }

    fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()> {
        self.scale_factor = info.scale_factor.max(1.0);
        self.resize(info.width, info.height)
    }

    fn resize(&mut self, lw: u32, lh: u32) -> AureaResult<()> {
        self.has_frame = false;
        self.logical_width = lw;
        self.logical_height = lh;
        let (rw, rh) = Self::raster_dimensions(lw, lh, self.scale_factor);
        self.width = rw;
        self.height = rh;

        let new_len = (rw * rh) as usize;
        self.frame_buffer.clear();
        if new_len > self.frame_buffer.capacity() {
            self.frame_buffer
                .reserve(new_len - self.frame_buffer.capacity());
        }
        self.frame_buffer.resize(new_len, 0);

        self.display_list.clear();
        self.prev_items.clear();
        self.tile_hashes.clear();
        self.last_frame_damage = None;
        Ok(())
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext + '_>> {
        self.display_list.clear();
        let mut ctx = RecordingContext::new(
            &mut self.display_list,
            self.logical_width,
            self.logical_height,
        );
        ctx.set_scale_factor(self.scale_factor);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<FrameOutcome> {
        let (bw, bh) = (self.width, self.height);
        let pending = self.pending_damage.take();
        let diff = self.diff_damage();

        let Some(damage) = resolve_frame_damage(pending, diff, bw, bh) else {
            return Ok(FrameOutcome::Unchanged);
        };

        let (tiles_x, tiles_y) = tile_grid_dims(bw, bh);
        let dirty_tiles = self.compute_dirty_tiles(damage, pending, tiles_x, tiles_y);

        self.last_frame_damage = union_dirty_tile_rects(&dirty_tiles, tiles_x, tiles_y, bw, bh);

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
        mark_occluded(
            items,
            (tiles_x, tiles_y, bw, bh),
            &mut self.scratch_covered,
            &mut occluded,
        );
        for (i, item) in items.iter().enumerate() {
            if let DrawCommand::Clear(color) = &item.command {
                clear_dirty_tiles(
                    &mut self.frame_buffer,
                    *color,
                    &dirty_tiles,
                    tiles_x,
                    tiles_y,
                    bw,
                    bh,
                    surface_clip.intersect_opt(item.clip),
                );
                continue;
            }

            if !should_render_item(item, &occluded, i, &dirty_tiles, tiles_x, tiles_y) {
                continue;
            }
            Self::render_item(
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

#[allow(clippy::too_many_arguments)]
fn clear_dirty_tiles(
    frame_buffer: &mut [u32],
    color: Color,
    dirty_tiles: &[bool],
    tiles_x: u32,
    tiles_y: u32,
    bw: u32,
    bh: u32,
    clip: ClipBox,
) {
    let c = color_to_u32(color);
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            if dirty_tiles[(ty * tiles_x + tx) as usize] {
                let rect = tile_rect(tx, ty, bw, bh);
                CpuRasterizer::clear_rect(&rect, c, frame_buffer, bw, clip);
            }
        }
    }
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
            &mut Vec::new(),
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
        let (mut covered, mut occluded) = (Vec::new(), Vec::new());
        mark_occluded(items, (TILES_X, TILES_Y, W, H), &mut covered, &mut occluded);
        occluded
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

    fn frame_at(scale: f32, draw: impl FnOnce(&mut dyn DrawingContext)) -> CpuRasterizer {
        let mut r = CpuRasterizer::new(40, 40);
        r.init(
            Surface::Cpu,
            SurfaceInfo {
                width: 40,
                height: 40,
                scale_factor: scale,
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
}
