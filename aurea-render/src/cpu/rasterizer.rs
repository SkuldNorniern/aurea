//! CPU rasterizer — flat framebuffer, no tile overhead.
//!
//! Renders the display list directly into a single `Vec<u32>` at physical
//! (HiDPI-scaled) resolution.  The buffer pointer is handed to the platform
//! layer without a copy; on macOS the canvas view stores it as a raw pointer
//! (safe: everything runs on the main thread, the pointer is updated before
//! each `setNeedsDisplay`).

use super::super::display_list::{CacheKey, DisplayList};
use super::super::renderer::{DrawingContext, Renderer};
use super::super::surface::{Surface, SurfaceInfo};
use super::super::types::{
    BlendMode, Color, GlyphMask, GradientStop, Image, LinearGradient, Paint, PaintStyle, Point,
    RadialGradient, Rect,
};
use super::blend::{blend_pixel, linear_to_srgb_u8, srgb_to_linear};
use super::context::CpuDrawingContext;
use super::path::{tessellate_path_into, Edge};
use super::scanline::fill_scanline;
use aurea_foundation::AureaResult;

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
    /// Reused across `draw_path` calls to avoid a `Vec` allocation per path per frame.
    scratch_edges: Vec<Edge>,
    /// Reused across `fill_scanline` calls to avoid a `Vec` allocation per scanline.
    scratch_xs: Vec<f32>,
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
            scratch_edges: Vec::new(),
            scratch_xs: Vec::new(),
        }
    }

    fn raster_dimensions(lw: u32, lh: u32, scale: f32) -> (u32, u32) {
        let s = scale.max(1.0);
        (
            ((lw as f32 * s).round() as u32).max(1),
            ((lh as f32 * s).round() as u32).max(1),
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
        let idx = y as u32 * w + x as u32;
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

    fn render_item(
        item: &super::super::display_list::DisplayItem,
        damage: Option<&Rect>,
        scale: f32,
        buf: &mut Vec<u32>,
        scratch_edges: &mut Vec<Edge>,
        scratch_xs: &mut Vec<f32>,
        bw: u32,
        bh: u32,
    ) -> AureaResult<()> {
        use super::super::command::DrawCommand;
        match &item.command {
            DrawCommand::Clear(color) => {
                let c = color_to_u32(*color);
                if let Some(rect) = damage {
                    Self::clear_rect(rect, c, buf, bw, bh);
                } else {
                    buf.fill(c);
                }
            }
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
                    bw,
                    bh,
                )?;
            }
            DrawCommand::DrawGlyphMask(mask, origin, color) => {
                Self::draw_glyph(mask, *origin, *color, buf, bw, bh);
            }
            DrawCommand::DrawImageRect(image, dest) => {
                let src = Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
                Self::draw_image(image, src, *dest, item.blend_mode, buf, bw, bh);
            }
            DrawCommand::DrawImageRegion(image, src, dest) => {
                Self::draw_image(image, *src, *dest, item.blend_mode, buf, bw, bh);
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
        let x0 = rect.x.floor().max(0.0).min(bw as f32) as u32;
        let y0 = rect.y.floor().max(0.0).min(bh as f32) as u32;
        let x1 = (rect.x + rect.width).ceil().max(0.0).min(bw as f32) as u32;
        let y1 = (rect.y + rect.height).ceil().max(0.0).min(bh as f32) as u32;

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
        buf: &mut Vec<u32>,
        bw: u32,
        bh: u32,
    ) {
        let x0 = (rect.x.max(0.0) as u32).min(bw);
        let y0 = (rect.y.max(0.0) as u32).min(bh);
        let x1 = ((rect.x + rect.width).ceil() as u32).min(bw);
        let y1 = ((rect.y + rect.height).ceil() as u32).min(bh);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        match paint.style {
            PaintStyle::Fill => {
                let c = color_to_u32(paint.color);
                if paint.color.a == 255 && mode == BlendMode::Normal {
                    // Fast path: opaque fill — one memset per row, no per-pixel math.
                    for y in y0..y1 {
                        let start = (y * bw + x0) as usize;
                        let end = (y * bw + x1) as usize;
                        if end <= buf.len() {
                            buf[start..end].fill(c);
                        }
                    }
                } else {
                    // Translucent fill: rect coverage is separable
                    // (cov(x,y) = cov_x(x) * cov_y(y)), so split into a
                    // fully-covered interior span (bulk fill/blend) plus
                    // edge rows/columns with per-axis coverage — mirroring
                    // the circle-fill fast path.
                    let xl = rect.x;
                    let xr = rect.x + rect.width;
                    let yl = rect.y;
                    let yr = rect.y + rect.height;

                    let cov_x = |x: u32| -> f32 {
                        ((x as f32 + 1.0).min(xr) - (x as f32).max(xl)).clamp(0.0, 1.0)
                    };
                    let cov_y = |y: u32| -> f32 {
                        ((y as f32 + 1.0).min(yr) - (y as f32).max(yl)).clamp(0.0, 1.0)
                    };

                    let xi0 = (xl.ceil() as i32).clamp(x0 as i32, x1 as i32) as u32;
                    let xi1 = (xr.floor() as i32).clamp(x0 as i32, x1 as i32) as u32;
                    let has_full_x = xi0 < xi1;
                    let yi0 = (yl.ceil() as i32).clamp(y0 as i32, y1 as i32) as u32;
                    let yi1 = (yr.floor() as i32).clamp(y0 as i32, y1 as i32) as u32;

                    let c_full = color_to_u32(paint.color);
                    let opaque_fast = mode == BlendMode::Normal && paint.color.a == 255;

                    for y in y0..y1 {
                        let row_cov = if y >= yi0 && y < yi1 { 1.0 } else { cov_y(y) };
                        if row_cov <= 0.0 {
                            continue;
                        }
                        let row_start = (y * bw) as usize;

                        if has_full_x {
                            for x in x0..xi0 {
                                let cov = cov_x(x) * row_cov;
                                if cov > 0.0 {
                                    let c = color_to_u32_with_coverage(paint.color, cov);
                                    Self::buf_set(buf, bw, x as i32, y as i32, c, mode);
                                }
                            }
                            if row_cov >= 1.0 {
                                if opaque_fast {
                                    buf[row_start + xi0 as usize..row_start + xi1 as usize]
                                        .fill(c_full);
                                } else {
                                    for x in xi0..xi1 {
                                        Self::buf_set(buf, bw, x as i32, y as i32, c_full, mode);
                                    }
                                }
                            } else {
                                let c = color_to_u32_with_coverage(paint.color, row_cov);
                                for x in xi0..xi1 {
                                    Self::buf_set(buf, bw, x as i32, y as i32, c, mode);
                                }
                            }
                            for x in xi1..x1 {
                                let cov = cov_x(x) * row_cov;
                                if cov > 0.0 {
                                    let c = color_to_u32_with_coverage(paint.color, cov);
                                    Self::buf_set(buf, bw, x as i32, y as i32, c, mode);
                                }
                            }
                        } else {
                            for x in x0..x1 {
                                let cov = cov_x(x) * row_cov;
                                if cov > 0.0 {
                                    let c = color_to_u32_with_coverage(paint.color, cov);
                                    Self::buf_set(buf, bw, x as i32, y as i32, c, mode);
                                }
                            }
                        }
                    }
                }
            }
            PaintStyle::Stroke => {
                let sw = paint.stroke_width as u32;
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
        }
    }

    fn draw_circle(
        center: Point,
        radius: f32,
        paint: &Paint,
        mode: BlendMode,
        buf: &mut Vec<u32>,
        bw: u32,
        bh: u32,
    ) {
        let x0 = ((center.x - radius).floor().max(0.0) as u32).min(bw);
        let y0 = ((center.y - radius).floor().max(0.0) as u32).min(bh);
        let x1 = ((center.x + radius).ceil() as u32).min(bw);
        let y1 = ((center.y + radius).ceil() as u32).min(bh);

        match paint.style {
            PaintStyle::Fill => {
                // Per row, compute the analytically fully-covered span and
                // memset/blend it directly; only the 1-2 pixels at each end
                // need the per-pixel sqrt-based coverage test.
                let r_in = (radius - 0.5).max(0.0);
                let r_out = radius + 0.5;
                let c_full = color_to_u32(paint.color);
                let opaque_fast = mode == BlendMode::Normal && paint.color.a == 255;

                for y in y0..y1 {
                    let dy = y as f32 + 0.5 - center.y;
                    if dy.abs() >= r_out {
                        continue;
                    }
                    let half_out = (r_out * r_out - dy * dy).max(0.0).sqrt();
                    let xo0 = ((center.x - half_out).floor().max(x0 as f32) as i32).min(x1 as i32);
                    let xo1 = ((center.x + half_out).ceil().min(x1 as f32) as i32).max(x0 as i32);

                    let (xi0, xi1) = if dy.abs() < r_in {
                        let half_in = (r_in * r_in - dy * dy).max(0.0).sqrt();
                        let a = (center.x - half_in).ceil() as i32;
                        let b = (center.x + half_in).floor() as i32;
                        if a < b {
                            (a.clamp(xo0, xo1), b.clamp(xo0, xo1))
                        } else {
                            (xo0, xo0)
                        }
                    } else {
                        (xo0, xo0)
                    };

                    // Left edge pixels (partial coverage).
                    for x in xo0..xi0 {
                        let cov = circle_coverage(center, radius, x as f32, y as f32);
                        if cov > 0.0 {
                            let c = color_to_u32_with_coverage(paint.color, cov);
                            Self::buf_set(buf, bw, x, y as i32, c, mode);
                        }
                    }
                    // Fully-covered interior span.
                    if xi0 < xi1 {
                        if opaque_fast {
                            let row_start = (y * bw) as usize;
                            buf[row_start + xi0 as usize..row_start + xi1 as usize].fill(c_full);
                        } else {
                            for x in xi0..xi1 {
                                Self::buf_set(buf, bw, x, y as i32, c_full, mode);
                            }
                        }
                    }
                    // Right edge pixels (partial coverage).
                    for x in xi1..xo1 {
                        let cov = circle_coverage(center, radius, x as f32, y as f32);
                        if cov > 0.0 {
                            let c = color_to_u32_with_coverage(paint.color, cov);
                            Self::buf_set(buf, bw, x, y as i32, c, mode);
                        }
                    }
                }
            }
            PaintStyle::Stroke => {
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
        }
    }

    fn draw_path(
        path: &super::super::types::Path,
        paint: &Paint,
        mode: BlendMode,
        scale: f32,
        buf: &mut Vec<u32>,
        scratch_edges: &mut Vec<Edge>,
        scratch_xs: &mut Vec<f32>,
        bw: u32,
        bh: u32,
    ) -> AureaResult<()> {
        tessellate_path_into(path, scale, scratch_edges);
        if scratch_edges.is_empty() {
            return Ok(());
        }

        let y_min = scratch_edges.iter().map(|e| e.y_min).fold(f32::MAX, f32::min);
        let y_max = scratch_edges.iter().map(|e| e.y_max).fold(f32::MIN, f32::max);
        let y_start = y_min.max(0.0).ceil() as u32;
        let y_end = y_max.min(bh as f32).ceil() as u32;

        for y in y_start..y_end {
            fill_scanline(
                scratch_edges,
                y as f32,
                buf,
                bw,
                bh,
                0,
                0,
                paint.color,
                mode,
                scratch_xs,
            );
        }
        Ok(())
    }

    fn draw_glyph(
        mask: &GlyphMask,
        origin: Point,
        color: Color,
        buf: &mut Vec<u32>,
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
            | ((color.r as u32) << 16)
            | ((color.g as u32) << 8)
            | color.b as u32;
        let dx = origin.x.round() as i32;
        let dy = origin.y.round() as i32;

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
            let row = (my as u32 * mask.width) as usize;
            let cov_row = &mask.coverage[row * 3..(row + mask.width as usize) * 3];
            let buf_row = (py as u32 * bw) as usize;

            for mx in x_lo..x_hi {
                let ci = mx as usize * 3;
                let cr8 = cov_row[ci];
                let cg8 = cov_row[ci + 1];
                let cb8 = cov_row[ci + 2];
                if cr8 == 0 && cg8 == 0 && cb8 == 0 {
                    continue;
                }

                let idx = buf_row + (dx + mx) as usize;

                if cr8 == 255 && cg8 == 255 && cb8 == 255 {
                    buf[idx] = opaque_pixel;
                    continue;
                }

                let cr = cr8 as f32 / 255.0;
                let cg = cg8 as f32 / 255.0;
                let cb = cb8 as f32 / 255.0;

                let dst = buf[idx];
                let da = (dst >> 24) & 0xff;
                let dr = ((dst >> 16) & 0xff) as u8;
                let dg = ((dst >> 8) & 0xff) as u8;
                let db = (dst & 0xff) as u8;

                let or_ = linear_to_srgb_u8(tr * cr + srgb_to_linear(dr) * (1.0 - cr));
                let og = linear_to_srgb_u8(tg * cg + srgb_to_linear(dg) * (1.0 - cg));
                let ob = linear_to_srgb_u8(tb * cb + srgb_to_linear(db) * (1.0 - cb));
                let cmax = cr.max(cg).max(cb);
                let sa = (cmax * 255.0).round() as u32;
                let oa = sa + ((255 - sa) * da) / 255;
                buf[idx] = (oa << 24) | (or_ << 16) | (og << 8) | ob;
            }
        }
    }

    fn draw_image(
        image: &Image,
        src: Rect,
        dest: Rect,
        mode: BlendMode,
        buf: &mut Vec<u32>,
        bw: u32,
        bh: u32,
    ) {
        if image.data.is_empty() || dest.width <= 0.0 || dest.height <= 0.0 {
            return;
        }
        let x0 = dest.x.max(0.0).ceil() as i32;
        let y0 = dest.y.max(0.0).ceil() as i32;
        let x1 = (dest.x + dest.width).min(bw as f32).floor() as i32;
        let y1 = (dest.y + dest.height).min(bh as f32).floor() as i32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let max_sx = image.width as f32 - 0.001;
        let max_sy = image.height as f32 - 0.001;

        // Unscaled 1:1 copy: skip the per-pixel division entirely and walk
        // the source row left-to-right with a plain offset.
        if (src.width - dest.width).abs() < 0.001 && (src.height - dest.height).abs() < 0.001 {
            let sx0 = (x0 as f32 - dest.x) + src.x;
            let mut row_buf = vec![0u32; (x1 - x0) as usize];
            for cy in y0..y1 {
                let v = (cy as f32 - dest.y) + src.y;
                let sy = v.clamp(0.0, max_sy) as u32;
                let src_row = &image.data[sy as usize * image.width as usize * 4..];
                let mut all_opaque = true;
                for (i, slot) in row_buf.iter_mut().enumerate() {
                    let sx = (sx0 + i as f32).clamp(0.0, max_sx) as usize;
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
                    *slot = ((a as u32) << 24)
                        | ((src_row[ii] as u32) << 16)
                        | ((src_row[ii + 1] as u32) << 8)
                        | (src_row[ii + 2] as u32);
                }
                if mode == BlendMode::Normal && all_opaque {
                    let row_start = (cy as u32 * bw + x0 as u32) as usize;
                    buf[row_start..row_start + row_buf.len()].copy_from_slice(&row_buf);
                } else {
                    for (i, &c) in row_buf.iter().enumerate() {
                        Self::buf_set(buf, bw, x0 + i as i32, cy, c, mode);
                    }
                }
            }
            return;
        }

        for cy in y0..y1 {
            let v = (cy as f32 - dest.y) / dest.height * src.height + src.y;
            let sy = v.clamp(0.0, max_sy) as u32;
            let src_row = &image.data[sy as usize * image.width as usize * 4..];
            for cx in x0..x1 {
                let u = (cx as f32 - dest.x) / dest.width * src.width + src.x;
                let sx = u.clamp(0.0, max_sx) as u32;
                let ii = sx as usize * 4;
                if ii + 3 >= src_row.len() {
                    continue;
                }
                let rgba = ((src_row[ii + 3] as u32) << 24)
                    | ((src_row[ii] as u32) << 16)
                    | ((src_row[ii + 1] as u32) << 8)
                    | (src_row[ii + 2] as u32);
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
            return ((c.a as u32) << 24)
                | ((c.r as u32) << 16)
                | ((c.g as u32) << 8)
                | (c.b as u32);
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
                let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * s).round() as u8;
                let (r, g, b_, a_) = (
                    lerp(c0.r, c1.r),
                    lerp(c0.g, c1.g),
                    lerp(c0.b, c1.b),
                    lerp(c0.a, c1.a),
                );
                return ((a_ as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b_ as u32);
            }
        }
        let c = if t <= stops[0].offset {
            stops[0].color
        } else {
            stops.last().unwrap().color
        };
        ((c.a as u32) << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
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
        buf: &mut Vec<u32>,
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
        let x0 = rect.x.max(0.0).ceil() as i32;
        let y0 = rect.y.max(0.0).ceil() as i32;
        let x1 = (rect.x + rect.width).min(bw as f32).floor() as i32;
        let y1 = (rect.y + rect.height).min(bh as f32).floor() as i32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        // `t` advances linearly with `cx`, so step it incrementally instead
        // of recomputing the full dot product for every pixel.
        let dt_x = dx / len_sq;
        let opaque_normal = mode == BlendMode::Normal;

        for cy in y0..y1 {
            let row = (cy as u32 * bw) as usize;
            let mut t = ((x0 as f32 + 0.5 - grad.start.x) * dx
                + (cy as f32 + 0.5 - grad.start.y) * dy)
                / len_sq;
            for cx in x0..x1 {
                let t_idx = (t.clamp(0.0, 1.0) * 255.0).round() as usize;
                let src = lut[t_idx];
                let idx = row + cx as usize;
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
        buf: &mut Vec<u32>,
        bw: u32,
        bh: u32,
    ) {
        if grad.radius <= 0.0 {
            return;
        }
        let lut = Self::build_gradient_lut(&grad.stops);
        let x0 = rect.x.max(0.0).ceil() as i32;
        let y0 = rect.y.max(0.0).ceil() as i32;
        let x1 = (rect.x + rect.width).min(bw as f32).floor() as i32;
        let y1 = (rect.y + rect.height).min(bh as f32).floor() as i32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let inv_radius = 1.0 / grad.radius;
        let opaque_normal = mode == BlendMode::Normal;

        for cy in y0..y1 {
            let row = (cy as u32 * bw) as usize;
            let dy = cy as f32 + 0.5 - grad.center.y;
            let dy_sq = dy * dy;
            for cx in x0..x1 {
                let dx = cx as f32 + 0.5 - grad.center.x;
                let dist = (dx * dx + dy_sq).sqrt();
                let t = (dist * inv_radius).min(1.0);
                let t_idx = (t.clamp(0.0, 1.0) * 255.0).round() as usize;
                let src = lut[t_idx];
                let idx = row + cx as usize;
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

        let damage: Option<Rect> = match (pending, diff) {
            // Nothing was explicitly marked dirty and the display list is
            // positionally identical to last frame: skip rendering and
            // presentation entirely.
            (None, FrameDamage::Unchanged) => {
                use crate::renderer::CURRENT_BUFFER;
                CURRENT_BUFFER.with(|b| *b.borrow_mut() = None);
                return Ok(());
            }
            (None, FrameDamage::Full) | (Some(_), FrameDamage::Full) => None,
            (None, FrameDamage::Region(r)) => Some(round_out_clamp(r, bw, bh)),
            (Some(p), FrameDamage::Unchanged) => Some(p),
            (Some(p), FrameDamage::Region(r)) => {
                Some(round_out_clamp(union_rect(p, r), bw, bh))
            }
        };

        for item in self.display_list.items() {
            let has_known_bounds = item.bounds.width > 0.0 && item.bounds.height > 0.0;
            if has_known_bounds && damage.as_ref().is_some_and(|rect| !item.intersects(rect)) {
                continue;
            }
            Self::render_item(
                item,
                damage.as_ref(),
                self.scale_factor,
                &mut self.frame_buffer,
                &mut self.scratch_edges,
                &mut self.scratch_xs,
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

    fn cleanup(&mut self) {
        self.display_list.clear();
        self.pending_damage = None;
        self.prev_items.clear();
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

/// Smallest rect covering both `a` and `b`.
fn union_rect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.width).max(b.x + b.width);
    let y1 = (a.y + a.height).max(b.y + b.height);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Rounds a damage rect outward to whole pixels and clamps it to the buffer.
fn round_out_clamp(r: Rect, bw: u32, bh: u32) -> Rect {
    let x0 = r.x.floor().max(0.0);
    let y0 = r.y.floor().max(0.0);
    let x1 = (r.x + r.width).ceil().min(bw as f32);
    let y1 = (r.y + r.height).ceil().min(bh as f32);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

fn color_to_u32(c: Color) -> u32 {
    ((c.a as u32) << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}

fn color_to_u32_with_coverage(c: Color, cov: f32) -> u32 {
    let a = (c.a as f32 * cov).round().clamp(0.0, 255.0) as u32;
    (a << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
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

#[allow(dead_code)]
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
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &paint).unwrap();
        drop(ctx);
        r.end_frame().unwrap();
        assert!(CURRENT_BUFFER.with(|b| b.borrow().is_some()));

        // Simulate the platform layer consuming the published buffer.
        CURRENT_BUFFER.with(|b| *b.borrow_mut() = None);

        // Second frame: identical draw calls, no explicit damage set.
        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(10, 20, 30)).unwrap();
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &paint).unwrap();
        drop(ctx);
        r.end_frame().unwrap();

        // Unchanged scene: end_frame must not republish the buffer.
        assert!(CURRENT_BUFFER.with(|b| b.borrow().is_none()));
    }
}
