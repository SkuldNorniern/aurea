//! CPU rasterizer — flat framebuffer, no tile overhead.
//!
//! Renders the display list directly into a single `Vec<u32>` at physical
//! (HiDPI-scaled) resolution.  The buffer pointer is handed to the platform
//! layer without a copy; on macOS the canvas view stores it as a raw pointer
//! (safe: everything runs on the main thread, the pointer is updated before
//! each `setNeedsDisplay`).

use std::cmp::Ordering as CmpOrdering;

use crate::command::DrawCommand;
use crate::cpu::blend::{blend_pixel, linear_to_srgb_u8, srgb_to_linear};
use crate::cpu::context::CpuDrawingContext;
use crate::cpu::path::{Edge, tessellate_path_into};
use crate::cpu::scanline::fill_spans;
use crate::display_list::{CacheKey, DisplayItem, DisplayList};
use crate::numeric::{
    f32_to_i32_clamped, f32_to_u8_clamped, f32_to_u32_clamped, f32_to_usize_clamped,
};
use crate::renderer::{DrawingContext, Renderer};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::{
    BlendMode, Color, GlyphMask, GradientStop, Image, LinearGradient, Paint, PaintStyle, Path,
    Point, RadialGradient, Rect,
};
use aurea_foundation::AureaResult;

/// Side length of a tile in physical pixels. See plan.md P6-A stage 3.
const TILE_SIZE: u32 = 256;

/// Result of diffing the current display list against the previous frame's.
#[derive(Debug)]
enum FrameDamage {
    /// The list is positionally identical (same cache keys) to last frame.
    Unchanged,
    /// An item with unknown bounds changed; repaint everything.
    Full,
    /// Only this region changed.
    Region(Rect),
}

pub struct CpuRasterizer {
    /// Physical-resolution pixel buffer — the only pixel allocation.
    frame_buffer: Vec<u32>,
    width: u32,
    height: u32,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
    display_list: DisplayList,
    pending_damage: Option<Rect>,
    /// `(cache_key, bounds)` of each item from the previous frame's display
    /// list, in display order. Diffed positionally against the current
    /// frame's list in `end_frame` to compute damage automatically.
    prev_items: Vec<(CacheKey, Rect)>,
    /// Hash of the `(cache_key, bounds)` sequence of items intersecting each
    /// `TILE_SIZE`-px tile, from the last frame that recomputed it. Row-major,
    /// `ceil(width/TILE_SIZE) * ceil(height/TILE_SIZE)` entries. A length
    /// mismatch (first frame, or after a resize) forces a full recompute.
    tile_hashes: Vec<u64>,
    /// Reused across `draw_path` calls to avoid a `Vec` allocation per path per frame.
    scratch_edges: Vec<Edge>,
    /// Reused across `fill_scanline` calls to avoid a `Vec` allocation per scanline.
    scratch_xs: Vec<f32>,
    /// Reused by the 1:1 `draw_image` blit to avoid a `Vec` allocation per row.
    scratch_row: Vec<u32>,
    /// Active-edge indices for the AET path fill; reused to avoid per-path allocs.
    scratch_active: Vec<usize>,
    /// Physical-pixel rect that was actually repainted in the last `end_frame`.
    /// `None` = full frame (or first frame / after resize).
    last_frame_damage: Option<Rect>,
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
            scratch_edges: Vec::new(),
            scratch_xs: Vec::new(),
            scratch_row: Vec::new(),
            scratch_active: Vec::new(),
            last_frame_damage: None,
        }
    }

    fn raster_dimensions(lw: u32, lh: u32, scale: f32) -> (u32, u32) {
        let s = scale.max(1.0);
        (
            f32_to_u32_clamped((lw as f32 * s).round()).max(1),
            f32_to_u32_clamped((lh as f32 * s).round()).max(1),
        )
    }

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

    // ── pixel helpers ────────────────────────────────────────────────────────

    #[inline]
    fn buf_set(buf: &mut [u32], w: u32, x: i32, y: i32, color: u32, mode: BlendMode) {
        if x < 0 || y < 0 {
            return;
        }
        let idx = y.cast_unsigned() * w + x.cast_unsigned();
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

    // ── rendering ────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn render_item(
        item: &DisplayItem,
        scale: f32,
        buf: &mut [u32],
        scratch_edges: &mut Vec<Edge>,
        scratch_xs: &mut Vec<f32>,
        scratch_row: &mut Vec<u32>,
        scratch_active: &mut Vec<usize>,
        bw: u32,
        bh: u32,
    ) -> AureaResult<()> {
        match &item.command {
            DrawCommand::DrawRect(rect, paint) => {
                Self::draw_rect(rect, paint, item.blend_mode, buf, bw, bh);
            }
            DrawCommand::DrawCircle(center, radius, paint) => {
                Self::draw_circle(*center, *radius, paint, item.blend_mode, buf, bw, bh);
            }
            DrawCommand::DrawPath(path, paint) => {
                Self::draw_path(
                    path,
                    paint,
                    item.blend_mode,
                    scale,
                    buf,
                    scratch_edges,
                    scratch_xs,
                    scratch_active,
                    bw,
                    bh,
                )?;
            }
            DrawCommand::DrawGlyphMask(mask, origin, color) => {
                Self::draw_glyph(mask, *origin, *color, buf, bw, bh);
            }
            DrawCommand::DrawImageRect(image, dest) => {
                let src = Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
                Self::draw_image(image, src, *dest, item.blend_mode, buf, scratch_row, bw, bh);
            }
            DrawCommand::DrawImageRegion(image, src, dest) => {
                Self::draw_image(
                    image,
                    *src,
                    *dest,
                    item.blend_mode,
                    buf,
                    scratch_row,
                    bw,
                    bh,
                );
            }
            DrawCommand::FillLinearGradient(grad, rect) => {
                Self::fill_linear_gradient(grad, *rect, item.blend_mode, buf, bw, bh);
            }
            DrawCommand::FillRadialGradient(grad, rect) => {
                Self::fill_radial_gradient(grad, *rect, item.blend_mode, buf, bw, bh);
            }
            _ => {}
        }
        Ok(())
    }

    fn clear_rect(rect: &Rect, color: u32, buf: &mut [u32], bw: u32, bh: u32) {
        let x0 = f32_to_u32_clamped(rect.x.floor().max(0.0).min(bw as f32));
        let y0 = f32_to_u32_clamped(rect.y.floor().max(0.0).min(bh as f32));
        let x1 = f32_to_u32_clamped((rect.x + rect.width).ceil().max(0.0).min(bw as f32));
        let y1 = f32_to_u32_clamped((rect.y + rect.height).ceil().max(0.0).min(bh as f32));

        for y in y0..y1 {
            let start = (y * bw + x0) as usize;
            let end = (y * bw + x1) as usize;
            buf[start..end].fill(color);
        }
    }

    fn draw_rect(rect: &Rect, paint: &Paint, mode: BlendMode, buf: &mut [u32], bw: u32, bh: u32) {
        let x0 = f32_to_u32_clamped(rect.x.max(0.0)).min(bw);
        let y0 = f32_to_u32_clamped(rect.y.max(0.0)).min(bh);
        let x1 = f32_to_u32_clamped((rect.x + rect.width).ceil()).min(bw);
        let y1 = f32_to_u32_clamped((rect.y + rect.height).ceil()).min(bh);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        match paint.style {
            PaintStyle::Fill => Self::fill_rect_region(rect, paint, mode, buf, bw, x0, y0, x1, y1),
            PaintStyle::Stroke => Self::stroke_rect_region(paint, mode, buf, bw, x0, y0, x1, y1),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_rect_region(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        if paint.color.a == 255 && mode == BlendMode::Normal {
            // Fast path: opaque fill — one memset per row, no per-pixel math.
            let c = color_to_u32(paint.color);
            for y in y0..y1 {
                let start = (y * bw + x0) as usize;
                let end = (y * bw + x1) as usize;
                if end <= buf.len() {
                    buf[start..end].fill(c);
                }
            }
        } else {
            Self::fill_rect_region_translucent(rect, paint, mode, buf, bw, x0, y0, x1, y1);
        }
    }

    // Translucent fill: rect coverage is separable (cov(x,y) = cov_x(x) *
    // cov_y(y)), so split into a fully-covered interior span (bulk fill/blend)
    // plus edge rows/columns with per-axis coverage — mirroring the
    // circle-fill fast path.
    #[allow(clippy::too_many_arguments)]
    fn fill_rect_region_translucent(
        rect: &Rect,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
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
            .cast_unsigned();
        let xi1 = f32_to_i32_clamped(xr.floor())
            .clamp(x0 as i32, x1 as i32)
            .cast_unsigned();
        let has_full_x = xi0 < xi1;
        let yi0 = f32_to_i32_clamped(yl.ceil())
            .clamp(y0 as i32, y1 as i32)
            .cast_unsigned();
        let yi1 = f32_to_i32_clamped(yr.floor())
            .clamp(y0 as i32, y1 as i32)
            .cast_unsigned();

        let ctx = RectFillCtx {
            paint,
            mode,
            xl,
            xr,
            c_full: color_to_u32(paint.color),
            opaque_fast: mode == BlendMode::Normal && paint.color.a == 255,
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
                    Self::buf_set(buf, bw, x as i32, y as i32, ctx.c_full, ctx.mode);
                }
            }
        } else {
            let c = color_to_u32_with_coverage(ctx.paint.color, row_cov);
            for x in xi0..xi1 {
                Self::buf_set(buf, bw, x as i32, y as i32, c, ctx.mode);
            }
        }

        Self::fill_rect_edge_span(buf, bw, ctx, xi1, x1, y, row_cov);
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
            if cov > 0.0 {
                let c = color_to_u32_with_coverage(ctx.paint.color, cov);
                Self::buf_set(buf, bw, x as i32, y as i32, c, ctx.mode);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stroke_rect_region(
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        let sw = f32_to_u32_clamped(paint.stroke_width);
        if sw == 0 {
            return;
        }
        let c = color_to_u32(paint.color);
        // top/bottom rows
        for x in x0..x1 {
            for dy in 0..sw.min(y1 - y0) {
                Self::buf_set(buf, bw, x as i32, (y0 + dy) as i32, c, mode);
                let bot = (y1 - 1).saturating_sub(dy);
                if bot >= y0 {
                    Self::buf_set(buf, bw, x as i32, bot as i32, c, mode);
                }
            }
        }
        // left/right columns
        for y in y0..y1 {
            for dx in 0..sw.min(x1 - x0) {
                Self::buf_set(buf, bw, (x0 + dx) as i32, y as i32, c, mode);
                let right = (x1 - 1).saturating_sub(dx);
                if right >= x0 {
                    Self::buf_set(buf, bw, right as i32, y as i32, c, mode);
                }
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
        bh: u32,
    ) {
        let x0 = f32_to_u32_clamped((center.x - radius).floor().max(0.0)).min(bw);
        let y0 = f32_to_u32_clamped((center.y - radius).floor().max(0.0)).min(bh);
        let x1 = f32_to_u32_clamped((center.x + radius).ceil()).min(bw);
        let y1 = f32_to_u32_clamped((center.y + radius).ceil()).min(bh);

        match paint.style {
            PaintStyle::Fill => {
                Self::fill_circle_region(center, radius, paint, mode, buf, bw, x0, y0, x1, y1);
            }
            PaintStyle::Stroke => {
                Self::stroke_circle_region(center, radius, paint, mode, buf, bw, x0, y0, x1, y1);
            }
        }
    }

    // Per row, compute the analytically fully-covered span and memset/blend
    // it directly; only the 1-2 pixels at each end need the per-pixel
    // sqrt-based coverage test.
    #[allow(clippy::too_many_arguments)]
    fn fill_circle_region(
        center: Point,
        radius: f32,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut [u32],
        bw: u32,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) {
        let ctx = CircleFillCtx {
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

        // Left edge pixels (partial coverage).
        Self::fill_circle_edge_span(buf, bw, ctx, xo0, xi0, y);

        // Fully-covered interior span.
        if xi0 < xi1 {
            if ctx.opaque_fast {
                let row_start = (y * bw) as usize;
                buf[row_start + xi0.cast_unsigned() as usize
                    ..row_start + xi1.cast_unsigned() as usize]
                    .fill(ctx.c_full);
            } else {
                for x in xi0..xi1 {
                    Self::buf_set(buf, bw, x, y as i32, ctx.c_full, ctx.mode);
                }
            }
        }

        // Right edge pixels (partial coverage).
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
                Self::buf_set(buf, bw, x, y as i32, c, ctx.mode);
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
                    Self::buf_set(buf, bw, x as i32, y as i32, c, mode);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_path(
        path: &Path,
        paint: &Paint,
        mode: BlendMode,
        scale: f32,
        buf: &mut [u32],
        scratch_edges: &mut Vec<Edge>,
        scratch_xs: &mut Vec<f32>,
        scratch_active: &mut Vec<usize>,
        bw: u32,
        bh: u32,
    ) -> AureaResult<()> {
        tessellate_path_into(path, scale, scratch_edges);
        if scratch_edges.is_empty() {
            return Ok(());
        }

        // Sort edges by y_min once so the sweep only looks at each edge when
        // it first becomes active — O(E·log E + A·rows) instead of O(E·rows).
        scratch_edges
            .sort_unstable_by(|a, b| a.y_min.partial_cmp(&b.y_min).unwrap_or(CmpOrdering::Equal));

        let y_min = scratch_edges[0].y_min;
        let y_max = scratch_edges
            .iter()
            .map(|e| e.y_max)
            .fold(f32::MIN, f32::max);
        let y_start = f32_to_u32_clamped(y_min.max(0.0).ceil());
        let y_end = f32_to_u32_clamped(y_max.min(bh as f32).ceil());

        scratch_active.clear();
        let mut enter_idx = 0usize;

        for y in y_start..y_end {
            let yf = y as f32;

            // Admit newly-active edges (all with y_min <= yf, in sorted order).
            while enter_idx < scratch_edges.len() && scratch_edges[enter_idx].y_min <= yf {
                scratch_active.push(enter_idx);
                enter_idx += 1;
            }

            // Retire edges whose y_max <= yf (they no longer cross this scanline).
            scratch_active.retain(|&i| scratch_edges[i].y_max > yf);

            if scratch_active.is_empty() {
                continue;
            }

            // Gather x crossings from the active set only, then sort and fill.
            scratch_xs.clear();
            for &i in scratch_active.iter() {
                scratch_xs.push(scratch_edges[i].x_at_y(yf));
            }
            scratch_xs.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(CmpOrdering::Equal));

            let row_base = y as usize * bw as usize;
            fill_spans(scratch_xs, row_base, buf, bw, 0, paint.color, mode);
        }
        Ok(())
    }

    fn draw_glyph(
        mask: &GlyphMask,
        origin: Point,
        color: Color,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
    ) {
        if mask.width == 0 || mask.height == 0 {
            return;
        }
        let tr = srgb_to_linear(color.r);
        let tg = srgb_to_linear(color.g);
        let tb = srgb_to_linear(color.b);
        // Fully-covered pixels composite to the text color at full alpha
        // regardless of the destination, so they can be written directly.
        let opaque_pixel = 0xFF00_0000
            | (u32::from(color.r) << 16)
            | (u32::from(color.g) << 8)
            | u32::from(color.b);
        let dx = f32_to_i32_clamped(origin.x.round());
        let dy = f32_to_i32_clamped(origin.y.round());

        let mw = mask.width as i32;
        let x_lo = (-dx).max(0);
        let x_hi = (bw as i32 - dx).min(mw);
        if x_lo >= x_hi {
            return;
        }

        for my in 0..mask.height as i32 {
            let py = dy + my;
            if py < 0 || py >= bh as i32 {
                continue;
            }
            let row = (my.cast_unsigned() * mask.width) as usize;
            let cov_row = &mask.coverage[row * 3..(row + mask.width as usize) * 3];
            let buf_row = (py.cast_unsigned() * bw) as usize;

            for mx in x_lo..x_hi {
                let ci = mx.cast_unsigned() as usize * 3;
                let cr8 = cov_row[ci];
                let cg8 = cov_row[ci + 1];
                let cb8 = cov_row[ci + 2];
                if cr8 == 0 && cg8 == 0 && cb8 == 0 {
                    continue;
                }

                let idx = buf_row + (dx + mx).cast_unsigned() as usize;

                if cr8 == 255 && cg8 == 255 && cb8 == 255 {
                    buf[idx] = opaque_pixel;
                    continue;
                }

                let cr = f32::from(cr8) / 255.0;
                let cg = f32::from(cg8) / 255.0;
                let cb = f32::from(cb8) / 255.0;

                let dst = buf[idx];
                let da = (dst >> 24) & 0xff;
                let dr = ((dst >> 16) & 0xff) as u8;
                let dg = ((dst >> 8) & 0xff) as u8;
                let db = (dst & 0xff) as u8;

                let or_ = linear_to_srgb_u8(tr * cr + srgb_to_linear(dr) * (1.0 - cr));
                let og = linear_to_srgb_u8(tg * cg + srgb_to_linear(dg) * (1.0 - cg));
                let ob = linear_to_srgb_u8(tb * cb + srgb_to_linear(db) * (1.0 - cb));
                let cmax = cr.max(cg).max(cb);
                let sa = f32_to_u32_clamped((cmax * 255.0).round());
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
        mode: BlendMode,
        buf: &mut [u32],
        scratch_row: &mut Vec<u32>,
        bw: u32,
        bh: u32,
    ) {
        if image.data.is_empty() || dest.width <= 0.0 || dest.height <= 0.0 {
            return;
        }
        let x0 = f32_to_i32_clamped(dest.x.max(0.0).ceil());
        let y0 = f32_to_i32_clamped(dest.y.max(0.0).ceil());
        let x1 = f32_to_i32_clamped((dest.x + dest.width).min(bw as f32).floor());
        let y1 = f32_to_i32_clamped((dest.y + dest.height).min(bh as f32).floor());
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        // Unscaled 1:1 copy: skip the per-pixel division entirely and walk
        // the source row left-to-right with a plain offset.
        if (src.width - dest.width).abs() < 0.001 && (src.height - dest.height).abs() < 0.001 {
            Self::draw_image_1to1(image, src, dest, mode, buf, scratch_row, bw, x0, y0, x1, y1);
        } else {
            Self::draw_image_scaled(image, src, dest, mode, buf, bw, x0, y0, x1, y1);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_image_1to1(
        image: &Image,
        src: Rect,
        dest: Rect,
        mode: BlendMode,
        buf: &mut [u32],
        scratch_row: &mut Vec<u32>,
        bw: u32,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
    ) {
        let max_sx = image.width as f32 - 0.001;
        let max_sy = image.height as f32 - 0.001;
        let sx0 = (x0 as f32 - dest.x) + src.x;
        // Reused scratch (one entry per destination column) — avoids a heap
        // allocation per row.
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
                let a = src_row[ii + 3];
                if a != 255 {
                    all_opaque = false;
                }
                *slot = (u32::from(a) << 24)
                    | (u32::from(src_row[ii]) << 16)
                    | (u32::from(src_row[ii + 1]) << 8)
                    | u32::from(src_row[ii + 2]);
            }
            if mode == BlendMode::Normal && all_opaque {
                let row_start = (cy.cast_unsigned() * bw + x0.cast_unsigned()) as usize;
                buf[row_start..row_start + row_buf.len()].copy_from_slice(row_buf.as_slice());
            } else {
                for (i, &c) in row_buf.iter().enumerate() {
                    let xi = i32::try_from(i).expect("row width fits in i32");
                    Self::buf_set(buf, bw, x0 + xi, cy, c, mode);
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
        buf: &mut [u32],
        bw: u32,
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
                let rgba = (u32::from(src_row[ii + 3]) << 24)
                    | (u32::from(src_row[ii]) << 16)
                    | (u32::from(src_row[ii + 1]) << 8)
                    | u32::from(src_row[ii + 2]);
                Self::buf_set(buf, bw, cx, cy, rgba, mode);
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

    /// Precompute 256 evenly-spaced gradient samples so the per-pixel loop
    /// does a table lookup instead of an O(stops) search through `stops`.
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
        bh: u32,
    ) {
        let dx = grad.end.x - grad.start.x;
        let dy = grad.end.y - grad.start.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq < 1e-10 {
            return;
        }
        let lut = Self::build_gradient_lut(&grad.stops);
        let x0 = f32_to_i32_clamped(rect.x.max(0.0).ceil());
        let y0 = f32_to_i32_clamped(rect.y.max(0.0).ceil());
        let x1 = f32_to_i32_clamped((rect.x + rect.width).min(bw as f32).floor());
        let y1 = f32_to_i32_clamped((rect.y + rect.height).min(bh as f32).floor());
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        // `t` advances linearly with `cx`, so step it incrementally instead
        // of recomputing the full dot product for every pixel.
        let dt_x = dx / len_sq;
        let opaque_normal = mode == BlendMode::Normal;

        for cy in y0..y1 {
            let row = (cy.cast_unsigned() * bw) as usize;
            let mut t = ((x0 as f32 + 0.5 - grad.start.x) * dx
                + (cy as f32 + 0.5 - grad.start.y) * dy)
                / len_sq;
            for cx in x0..x1 {
                let t_idx = f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round());
                let src = lut[t_idx];
                let idx = row + cx.cast_unsigned() as usize;
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
        bh: u32,
    ) {
        if grad.radius <= 0.0 {
            return;
        }
        let lut = Self::build_gradient_lut(&grad.stops);
        let x0 = f32_to_i32_clamped(rect.x.max(0.0).ceil());
        let y0 = f32_to_i32_clamped(rect.y.max(0.0).ceil());
        let x1 = f32_to_i32_clamped((rect.x + rect.width).min(bw as f32).floor());
        let y1 = f32_to_i32_clamped((rect.y + rect.height).min(bh as f32).floor());
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let inv_radius = 1.0 / grad.radius;
        let opaque_normal = mode == BlendMode::Normal;

        for cy in y0..y1 {
            let row = (cy.cast_unsigned() * bw) as usize;
            let dy = cy as f32 + 0.5 - grad.center.y;
            let dy_sq = dy * dy;
            for cx in x0..x1 {
                let dx = cx as f32 + 0.5 - grad.center.x;
                let dist = (dx * dx + dy_sq).sqrt();
                let t = (dist * inv_radius).min(1.0);
                let t_idx = f32_to_usize_clamped((t.clamp(0.0, 1.0) * 255.0).round());
                let src = lut[t_idx];
                let idx = row + cx.cast_unsigned() as usize;
                buf[idx] = if opaque_normal && (src >> 24) == 255 {
                    src
                } else {
                    blend_pixel(src, buf[idx], mode)
                };
            }
        }
    }

    /// Diffs the current display list against `prev_items` positionally to
    /// find what changed since the last frame. See plan.md P6-A stage 1.
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

    /// Determines which tiles need to be (re)painted this frame and updates
    /// `tile_hashes` for the next frame's comparison. See plan.md P6-A stage 3.
    ///
    /// `damage` is the stage-1 damage rect (`None` means "repaint
    /// everything"); `forced` is the raw `set_damage` hint, whose tiles are
    /// marked dirty unconditionally regardless of hash (a caller asking for a
    /// region to be redrawn may have a reason the cache-key hash can't see).
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

        let mut dirty = vec![false; tile_count];

        let range = match damage {
            Some(rect) => tile_range(rect, tiles_x, tiles_y),
            None => (0, 0, tiles_x, tiles_y),
        };
        self.refine_tile_hashes(range, (tiles_x, tiles_y), &mut dirty);

        if damage.is_none() {
            // Full damage: repaint every tile regardless of whether its hash
            // happens to match (it was just recomputed above either way).
            dirty.fill(true);
        }
        if let Some(rect) = forced {
            mark_tile_range_dirty(rect, tiles_x, tiles_y, &mut dirty);
        }
        self.propagate_dirty_tiles(tiles_x, tiles_y, &mut dirty);
        dirty
    }

    /// `render_item` paints an item's full bounds with no per-tile clipping,
    /// but only dirty tiles get `Clear`d this frame. If an item spans both a
    /// dirty and a clean tile, redrawing it paints over the clean tile's
    /// already-composited pixels too — for non-opaque content that compounds
    /// every frame it's redrawn, which is visible as flicker in regions that
    /// otherwise shouldn't be touched. Mark every tile an item overlaps as
    /// dirty once any one of them is, repeating to a fixed point (an item
    /// pulled in by this can itself drag in further items/tiles).
    fn propagate_dirty_tiles(&self, tiles_x: u32, tiles_y: u32, dirty: &mut [bool]) {
        let tile_count = (tiles_x * tiles_y) as usize;
        for _ in 0..tile_count {
            let mut changed = false;
            for item in self.display_list.items() {
                if !is_known_bounds(item.bounds) {
                    continue;
                }
                if item_overlaps_dirty_tiles(item.bounds, dirty, tiles_x, tiles_y) {
                    let (tx0, ty0, tx1, ty1) = tile_range(item.bounds, tiles_x, tiles_y);
                    for ty in ty0..ty1 {
                        for tx in tx0..tx1 {
                            let idx = (ty * tiles_x + tx) as usize;
                            if !dirty[idx] {
                                dirty[idx] = true;
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Recomputes the hash of every tile in `[tx0,tx1) x [ty0,ty1)` from the
    /// current display list and marks tiles whose hash changed as dirty.
    /// Tiles outside this range are untouched: stage 1's diff guarantees
    /// nothing intersecting them changed since the last frame.
    fn refine_tile_hashes(
        &mut self,
        (tx0, ty0, tx1, ty1): (u32, u32, u32, u32),
        (tiles_x, tiles_y): (u32, u32),
        dirty: &mut [bool],
    ) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        if tx0 >= tx1 || ty0 >= ty1 {
            return;
        }
        let range_w = (tx1 - tx0) as usize;
        let mut hashers: Vec<DefaultHasher> = (0..range_w * (ty1 - ty0) as usize)
            .map(|_| DefaultHasher::new())
            .collect();

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
    }

    /// Records `(cache_key, bounds)` for every item in the just-rendered
    /// display list, so the next frame's `diff_damage` can compare against it.
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

    fn resize(&mut self, lw: u32, lh: u32) -> AureaResult<()> {
        self.logical_width = lw;
        self.logical_height = lh;
        let (rw, rh) = Self::raster_dimensions(lw, lh, self.scale_factor);
        self.width = rw;
        self.height = rh;

        // Reuse the existing allocation when possible: clear + resize-in-place
        // instead of allocating a fresh buffer on every step of a live resize
        // drag. `reserve` amortizes growth (like Vec::push) when the new size
        // exceeds the current capacity.
        let new_len = (rw * rh) as usize;
        self.frame_buffer.clear();
        if new_len > self.frame_buffer.capacity() {
            self.frame_buffer
                .reserve(new_len - self.frame_buffer.capacity());
        }
        self.frame_buffer.resize(new_len, 0);

        self.display_list.clear();
        // The freshly-zeroed buffer no longer matches `prev_items`; the next
        // frame's diff would otherwise compare against stale positions/sizes.
        self.prev_items.clear();
        // The tile grid dimensions change with the buffer size, and the
        // freshly-cleared buffer no longer matches any cached tile hash.
        self.tile_hashes.clear();
        self.last_frame_damage = None;
        Ok(())
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        self.display_list.clear();
        let mut ctx = CpuDrawingContext::new(
            &mut self.display_list as *mut DisplayList,
            self.logical_width,
            self.logical_height,
        );
        ctx.set_scale_factor(self.scale_factor);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        let (bw, bh) = (self.width, self.height);
        let pending = self.pending_damage.take();
        let diff = self.diff_damage();

        // `None` means the display list is positionally identical to last
        // frame and nothing was explicitly marked dirty: skip rendering and
        // presentation entirely.
        let Some(damage) = resolve_frame_damage(pending, diff, bw, bh) else {
            use crate::renderer::CURRENT_BUFFER;
            CURRENT_BUFFER.with(|b| *b.borrow_mut() = None);
            return Ok(());
        };

        let (tiles_x, tiles_y) = tile_grid_dims(bw, bh);
        let dirty_tiles = self.compute_dirty_tiles(damage, pending, tiles_x, tiles_y);

        // Union of all dirty tile rects in physical pixels — exposed via
        // `last_frame_damage()` so the platform layer can do a partial IOSurface copy.
        self.last_frame_damage = union_dirty_tile_rects(&dirty_tiles, tiles_x, tiles_y, bw, bh);

        let items = self.display_list.items();
        for (i, item) in items.iter().enumerate() {
            // `Clear` conceptually covers the whole buffer, but only the
            // dirty tiles' pixels actually need to be overwritten — anything
            // outside them is already correct from a prior frame.
            if let DrawCommand::Clear(color) = &item.command {
                clear_dirty_tiles(
                    &mut self.frame_buffer,
                    *color,
                    &dirty_tiles,
                    tiles_x,
                    tiles_y,
                    bw,
                    bh,
                );
                continue;
            }

            if !should_render_item(item, items, i, &dirty_tiles, tiles_x, tiles_y) {
                continue;
            }
            Self::render_item(
                item,
                self.scale_factor,
                &mut self.frame_buffer,
                &mut self.scratch_edges,
                &mut self.scratch_xs,
                &mut self.scratch_row,
                &mut self.scratch_active,
                bw,
                bh,
            )?;
        }

        self.capture_prev_items();

        use crate::renderer::CURRENT_BUFFER;
        let (ptr, sz, w, h) = self.get_buffer();
        CURRENT_BUFFER.with(|b| *b.borrow_mut() = Some((ptr, sz, w, h)));
        Ok(())
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

// ── pixel math helpers ───────────────────────────────────────────────────────

/// Bounds are "known" if non-empty; `compute_bounds` returns `(0,0,0,0)` for
/// commands it can't size (e.g. clip/transform/opacity push-pop markers).
fn is_known_bounds(r: Rect) -> bool {
    r.width > 0.0 && r.height > 0.0
}

/// Resolves explicit (`forced`) and diffed (`diff`) damage into the rect that
/// must be repainted this frame. `Some(None)` means "repaint everything";
/// `None` means the frame is unchanged and rendering can be skipped entirely.
fn resolve_frame_damage(
    forced: Option<Rect>,
    diff: FrameDamage,
    bw: u32,
    bh: u32,
) -> Option<Option<Rect>> {
    match (forced, diff) {
        (None, FrameDamage::Unchanged) => None,
        (None, FrameDamage::Full) | (Some(_), FrameDamage::Full) => Some(None),
        (None, FrameDamage::Region(r)) => Some(Some(round_out_clamp(r, bw, bh))),
        (Some(p), FrameDamage::Unchanged) => Some(Some(p)),
        (Some(p), FrameDamage::Region(r)) => Some(Some(round_out_clamp(union_rect(p, r), bw, bh))),
    }
}

/// Union of all dirty tile rects in physical pixels.
fn union_dirty_tile_rects(
    dirty_tiles: &[bool],
    tiles_x: u32,
    tiles_y: u32,
    bw: u32,
    bh: u32,
) -> Option<Rect> {
    let mut acc: Option<Rect> = None;
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            if dirty_tiles[(ty * tiles_x + tx) as usize] {
                let tr = tile_rect(tx, ty, bw, bh);
                acc = Some(match acc {
                    None => tr,
                    Some(a) => union_rect(a, tr),
                });
            }
        }
    }
    acc
}

/// Overwrites the dirty-tile pixels of `frame_buffer` with `color`. `Clear`
/// conceptually covers the whole buffer, but only the dirty tiles' pixels
/// actually need to be overwritten — anything outside them is already
/// correct from a prior frame.
fn clear_dirty_tiles(
    frame_buffer: &mut [u32],
    color: Color,
    dirty_tiles: &[bool],
    tiles_x: u32,
    tiles_y: u32,
    bw: u32,
    bh: u32,
) {
    let c = color_to_u32(color);
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            if dirty_tiles[(ty * tiles_x + tx) as usize] {
                let rect = tile_rect(tx, ty, bw, bh);
                CpuRasterizer::clear_rect(&rect, c, frame_buffer, bw, bh);
            }
        }
    }
}

/// Whether `items[i]` needs rendering: it must have known bounds that
/// overlap a dirty tile, and must not be fully occluded by a later item.
fn should_render_item(
    item: &DisplayItem,
    items: &[DisplayItem],
    i: usize,
    dirty_tiles: &[bool],
    tiles_x: u32,
    tiles_y: u32,
) -> bool {
    if !is_known_bounds(item.bounds) {
        return true;
    }
    item_overlaps_dirty_tiles(item.bounds, dirty_tiles, tiles_x, tiles_y) && !is_occluded(items, i)
}

/// Smallest rect covering both `a` and `b`.
fn union_rect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.width).max(b.x + b.width);
    let y1 = (a.y + a.height).max(b.y + b.height);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Whether `outer` fully contains `inner`.
fn rect_contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// True if some later item in `items` is an opaque, normally-blended draw
/// whose bounds fully cover `items[i]`'s bounds — i.e. `items[i]` is
/// completely painted over and contributes nothing to the final frame.
/// `items[i].bounds` must be known (non-zero) bounds; callers check this.
fn is_occluded(items: &[DisplayItem], i: usize) -> bool {
    let bounds = items[i].bounds;
    items[i + 1..].iter().any(|later| {
        later.opaque && later.blend_mode == BlendMode::Normal && rect_contains(later.bounds, bounds)
    })
}

/// Number of `TILE_SIZE` tiles needed to cover a `bw x bh` buffer.
fn tile_grid_dims(bw: u32, bh: u32) -> (u32, u32) {
    (bw.div_ceil(TILE_SIZE), bh.div_ceil(TILE_SIZE))
}

/// The half-open range of tiles `[tx0,tx1) x [ty0,ty1)` that `bounds`
/// overlaps, clamped to the `tiles_x x tiles_y` grid.
fn tile_range(bounds: Rect, tiles_x: u32, tiles_y: u32) -> (u32, u32, u32, u32) {
    let tile = TILE_SIZE as f32;
    let x0 = f32_to_u32_clamped((bounds.x / tile).floor().max(0.0)).min(tiles_x);
    let y0 = f32_to_u32_clamped((bounds.y / tile).floor().max(0.0)).min(tiles_y);
    let x1 = f32_to_u32_clamped(((bounds.x + bounds.width) / tile).ceil().max(0.0))
        .min(tiles_x)
        .max(x0);
    let y1 = f32_to_u32_clamped(((bounds.y + bounds.height) / tile).ceil().max(0.0))
        .min(tiles_y)
        .max(y0);
    (x0, y0, x1, y1)
}

/// Pixel rect covered by tile `(tx, ty)`, clipped to the `bw x bh` buffer.
fn tile_rect(tx: u32, ty: u32, bw: u32, bh: u32) -> Rect {
    let x0 = (tx * TILE_SIZE) as f32;
    let y0 = (ty * TILE_SIZE) as f32;
    let x1 = ((tx + 1) * TILE_SIZE).min(bw) as f32;
    let y1 = ((ty + 1) * TILE_SIZE).min(bh) as f32;
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Whether any tile that `bounds` overlaps is marked dirty.
fn item_overlaps_dirty_tiles(bounds: Rect, dirty: &[bool], tiles_x: u32, tiles_y: u32) -> bool {
    let (tx0, ty0, tx1, ty1) = tile_range(bounds, tiles_x, tiles_y);
    (ty0..ty1).any(|ty| (tx0..tx1).any(|tx| dirty[(ty * tiles_x + tx) as usize]))
}

/// Marks every tile that `rect` overlaps as dirty.
fn mark_tile_range_dirty(rect: Rect, tiles_x: u32, tiles_y: u32, dirty: &mut [bool]) {
    let (tx0, ty0, tx1, ty1) = tile_range(rect, tiles_x, tiles_y);
    for ty in ty0..ty1 {
        for tx in tx0..tx1 {
            dirty[(ty * tiles_x + tx) as usize] = true;
        }
    }
}

/// Rounds a damage rect outward to whole pixels and clamps it to the buffer.
fn round_out_clamp(r: Rect, bw: u32, bh: u32) -> Rect {
    let x0 = r.x.floor().max(0.0);
    let y0 = r.y.floor().max(0.0);
    let x1 = (r.x + r.width).ceil().min(bw as f32);
    let y1 = (r.y + r.height).ceil().min(bh as f32);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// Shared per-call state for the circle-fill row/span helpers.
struct CircleFillCtx<'a> {
    center: Point,
    radius: f32,
    paint: &'a Paint,
    mode: BlendMode,
    r_in: f32,
    r_out: f32,
    c_full: u32,
    opaque_fast: bool,
}

/// Shared per-call state for the translucent rect-fill row/span helpers.
struct RectFillCtx<'a> {
    paint: &'a Paint,
    mode: BlendMode,
    xl: f32,
    xr: f32,
    c_full: u32,
    opaque_fast: bool,
}

fn rect_cov_x(x: u32, xl: f32, xr: f32) -> f32 {
    ((x as f32 + 1.0).min(xr) - (x as f32).max(xl)).clamp(0.0, 1.0)
}

fn rect_cov_y(y: u32, yl: f32, yr: f32) -> f32 {
    ((y as f32 + 1.0).min(yr) - (y as f32).max(yl)).clamp(0.0, 1.0)
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
mod diff_damage_tests {
    use super::*;
    use crate::command::DrawCommand;
    use crate::display_list::{DisplayItem, NodeId};

    fn item(key: u64, bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            NodeId(0),
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
        use crate::renderer::CURRENT_BUFFER;
        use crate::types::Paint;

        let mut r = CpuRasterizer::new(32, 32);
        let paint = Paint::new();

        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(10, 20, 30)).unwrap();
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &paint)
            .unwrap();
        drop(ctx);
        r.end_frame().unwrap();
        assert!(CURRENT_BUFFER.with(|b| b.borrow().is_some()));

        // Simulate the platform layer consuming the published buffer.
        CURRENT_BUFFER.with(|b| *b.borrow_mut() = None);

        // Second frame: identical draw calls, no explicit damage set.
        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(10, 20, 30)).unwrap();
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &paint)
            .unwrap();
        drop(ctx);
        r.end_frame().unwrap();

        // Unchanged scene: end_frame must not republish the buffer.
        assert!(CURRENT_BUFFER.with(|b| b.borrow().is_none()));
    }
}

#[cfg(test)]
mod occlusion_tests {
    use super::*;
    use crate::command::DrawCommand;
    use crate::display_list::{DisplayItem, NodeId};

    fn occluder_item(bounds: Rect, blend: BlendMode) -> DisplayItem {
        DisplayItem::new(
            NodeId(0),
            CacheKey::from_hash(1),
            bounds,
            true,
            blend,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    fn plain_item(bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            NodeId(0),
            CacheKey::from_hash(2),
            bounds,
            false,
            BlendMode::Normal,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
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
    fn later_opaque_normal_item_occludes_earlier() {
        let small = Rect::new(2.0, 2.0, 4.0, 4.0);
        let big = Rect::new(0.0, 0.0, 10.0, 10.0);
        let items = vec![plain_item(small), occluder_item(big, BlendMode::Normal)];
        assert!(is_occluded(&items, 0));
    }

    #[test]
    fn partial_cover_does_not_occlude() {
        let small = Rect::new(2.0, 2.0, 4.0, 4.0);
        let partial = Rect::new(0.0, 0.0, 5.0, 5.0);
        let items = vec![plain_item(small), occluder_item(partial, BlendMode::Normal)];
        assert!(!is_occluded(&items, 0));
    }

    #[test]
    fn non_normal_blend_does_not_occlude() {
        let small = Rect::new(2.0, 2.0, 4.0, 4.0);
        let big = Rect::new(0.0, 0.0, 10.0, 10.0);
        let items = vec![plain_item(small), occluder_item(big, BlendMode::Multiply)];
        assert!(!is_occluded(&items, 0));
    }

    #[test]
    fn earlier_item_does_not_occlude_later() {
        let small = Rect::new(2.0, 2.0, 4.0, 4.0);
        let big = Rect::new(0.0, 0.0, 10.0, 10.0);
        let items = vec![occluder_item(big, BlendMode::Normal), plain_item(small)];
        assert!(!is_occluded(&items, 0));
    }
}

#[cfg(test)]
mod tile_cache_tests {
    use super::*;
    use crate::command::DrawCommand;
    use crate::display_list::{DisplayItem, NodeId};
    use crate::types::Paint;

    fn item(key: u64, bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            NodeId(0),
            CacheKey::from_hash(key),
            bounds,
            false,
            BlendMode::Normal,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    #[test]
    fn tile_grid_dims_rounds_up() {
        assert_eq!(tile_grid_dims(256, 256), (1, 1));
        assert_eq!(tile_grid_dims(257, 256), (2, 1));
        assert_eq!(tile_grid_dims(512, 300), (2, 2));
    }

    #[test]
    fn tile_range_clamps_huge_bounds_to_grid() {
        let max = Rect::new(0.0, 0.0, f32::MAX, f32::MAX);
        assert_eq!(tile_range(max, 3, 2), (0, 0, 3, 2));
    }

    #[test]
    fn tile_range_picks_single_tile() {
        let bounds = Rect::new(300.0, 10.0, 4.0, 4.0);
        assert_eq!(tile_range(bounds, 4, 4), (1, 0, 2, 1));
    }

    #[test]
    fn changing_one_tiles_item_leaves_other_tile_clean() {
        let mut r = CpuRasterizer::new(512, 512); // 2x2 tile grid
        let a = Rect::new(4.0, 4.0, 8.0, 8.0); // tile (0,0)
        let b = Rect::new(300.0, 300.0, 8.0, 8.0); // tile (1,1)
        let full = Rect::new(0.0, 0.0, 512.0, 512.0);

        r.display_list.push(item(1, a));
        r.display_list.push(item(2, b));
        let dirty = r.compute_dirty_tiles(Some(full), None, 2, 2);
        assert!(dirty[0], "tile (0,0) dirty on first computation");
        assert!(dirty[3], "tile (1,1) dirty on first computation");

        // Second frame: only the item in tile (0,0) changes.
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
        let mut r = CpuRasterizer::new(512, 512); // 2x2 tile grid
        let a = Rect::new(4.0, 4.0, 8.0, 8.0); // tile (0,0)
        let forced = Rect::new(300.0, 300.0, 8.0, 8.0); // tile (1,1)
        let full = Rect::new(0.0, 0.0, 512.0, 512.0);

        r.display_list.push(item(1, a));
        let _ = r.compute_dirty_tiles(Some(full), None, 2, 2);

        // Same content as last frame, but the caller explicitly marked
        // tile (1,1)'s region dirty via `set_damage`.
        let dirty = r.compute_dirty_tiles(Some(full), Some(forced), 2, 2);
        assert!(!dirty[0], "tile (0,0) unchanged and not forced");
        assert!(dirty[3], "tile (1,1) forced dirty by set_damage");
    }

    #[test]
    fn item_spanning_a_dirty_tile_drags_in_its_other_tiles() {
        let mut r = CpuRasterizer::new(512, 512); // 2x2 tile grid
        let full = Rect::new(0.0, 0.0, 512.0, 512.0);
        // Spans tiles (0,0) and (1,0): x in [240, 280) crosses the x=256 tile edge.
        let spanning = Rect::new(240.0, 4.0, 40.0, 8.0);
        // Fully inside tile (0,0).
        let small = Rect::new(4.0, 4.0, 8.0, 8.0);

        r.display_list.push(item(100, spanning));
        r.display_list.push(item(1, small));
        let _ = r.compute_dirty_tiles(Some(full), None, 2, 2);

        // Second frame: only `small`'s cache key changes. Its own damage
        // region only covers tile (0,0), but `spanning` also overlaps that
        // tile, so its other tile (1,0) must be dragged in too — otherwise
        // redrawing `spanning` across both tiles would paint onto tile
        // (1,0) without it having been cleared this frame.
        r.display_list.clear();
        r.display_list.push(item(100, spanning));
        r.display_list.push(item(2, small));
        let dirty = r.compute_dirty_tiles(Some(full), None, 2, 2);
        assert!(dirty[0], "tile (0,0) dirty: small's cache key changed");
        assert!(
            dirty[1],
            "tile (1,0) dragged in: spanning item also covers it"
        );
        assert!(!dirty[2], "tile (0,1) untouched");
        assert!(!dirty[3], "tile (1,1) untouched");
    }

    #[test]
    fn unrelated_tile_pixels_survive_a_localized_redraw() {
        let mut r = CpuRasterizer::new(512, 512); // 2x2 tile grid
        let paint_a = Paint::new().color(Color::rgb(10, 20, 30));
        let paint_b = Paint::new().color(Color::rgb(200, 100, 50));
        let paint_c = Paint::new().color(Color::rgb(0, 255, 0));

        // Frame 1: background clear, plus one rect per occupied tile.
        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(1, 2, 3)).unwrap();
        // tile (0,0)
        ctx.draw_rect(Rect::new(4.0, 4.0, 8.0, 8.0), &paint_a)
            .unwrap();
        // tile (1,1)
        ctx.draw_rect(Rect::new(300.0, 300.0, 8.0, 8.0), &paint_b)
            .unwrap();
        drop(ctx);
        r.end_frame().unwrap();

        let bw = r.width;
        assert_eq!(
            pixel_at(&r.frame_buffer, bw, 304, 304),
            color_to_u32(paint_b.color)
        );

        // Frame 2: only the tile (0,0) rect changes color.
        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(1, 2, 3)).unwrap();
        ctx.draw_rect(Rect::new(4.0, 4.0, 8.0, 8.0), &paint_c)
            .unwrap();
        ctx.draw_rect(Rect::new(300.0, 300.0, 8.0, 8.0), &paint_b)
            .unwrap();
        drop(ctx);
        r.end_frame().unwrap();

        // Tile (0,0) reflects the new color.
        assert_eq!(
            pixel_at(&r.frame_buffer, bw, 8, 8),
            color_to_u32(paint_c.color)
        );
        // Tile (1,1) was never touched this frame; its pixels persist.
        assert_eq!(
            pixel_at(&r.frame_buffer, bw, 304, 304),
            color_to_u32(paint_b.color)
        );
    }
}
