//! CPU rasterizer with tile-based rendering.
//!
//! Turns display-list commands into pixels inside fixed-size tiles. Only tiles
//! that intersect the damage region are redrawn, so partial updates stay cheap.

use super::super::display_list::{DisplayItem, DisplayList};
use super::super::renderer::{DrawingContext, Renderer};
use super::super::surface::{Surface, SurfaceInfo};
use super::super::types::{
    BlendMode, Color, GradientStop, Image, LinearGradient, Paint, PaintStyle, Point,
    RadialGradient, Rect,
};
use super::blend::blend_pixel;
use super::cache::BoundedCache;
use super::context::CpuDrawingContext;
use super::path::tessellate_path;
use super::scanline::fill_scanline;
use super::tile::{TILE_SIZE, TileStore};
use crate::AureaResult;

/// Rasterizer that draws the display list into a tile grid for partial redraw.
pub struct CpuRasterizer {
    tile_store: TileStore,
    #[allow(dead_code)]
    cache: BoundedCache<Vec<u32>>,
    width: u32,
    height: u32,
    display_list: DisplayList,
    initialized: bool,
    scale_factor: f32,
    pending_damage: Option<Rect>,
}

impl CpuRasterizer {
    /// Creates a rasterizer for the given canvas size.
    pub fn new(width: u32, height: u32) -> Self {
        const CACHE_BYTES: usize = 16 * 1024 * 1024;
        Self {
            tile_store: TileStore::new(width, height),
            cache: BoundedCache::new(CACHE_BYTES),
            width,
            height,
            display_list: DisplayList::new(),
            initialized: false,
            scale_factor: 1.0,
            pending_damage: None,
        }
    }

    /// Sets the damage region for the next frame; if None, the whole canvas is redrawn.
    pub fn set_damage(&mut self, damage: Option<Rect>) {
        self.pending_damage = damage;
    }

    /// Resizes the rasterizer to new dimensions and clears the display list.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.tile_store.resize(width, height);
    }

    /// Renders the display list into tiles, updating only tiles that intersect the damage region.
    pub fn render(&mut self, display_list: &DisplayList, damage: &Rect) -> AureaResult<()> {
        self.tile_store.mark_damaged(damage);
        let dirty_tiles: Vec<_> = self.tile_store.dirty_tiles();
        for (tile_x, tile_y) in dirty_tiles {
            self.render_tile(tile_x, tile_y, display_list)?;
        }

        Ok(())
    }

    /// Renders one tile by clearing it and drawing all display items that intersect it.
    fn render_tile(
        &mut self,
        tile_x: u32,
        tile_y: u32,
        display_list: &DisplayList,
    ) -> AureaResult<()> {
        let tile_bounds = Rect::new(
            (tile_x * TILE_SIZE) as f32,
            (tile_y * TILE_SIZE) as f32,
            TILE_SIZE as f32,
            TILE_SIZE as f32,
        );

        let intersecting_items: Vec<_> = display_list
            .items()
            .iter()
            .filter(|item| item.intersects(&tile_bounds))
            .collect();

        if let Some(tile) = self.tile_store.get_tile_mut(tile_x, tile_y) {
            tile.clear(0);
            for item in intersecting_items {
                Self::render_item_to_tile_static(item, tile, &tile_bounds)?;
            }

            tile.mark_clean();
        }

        Ok(())
    }

    /// Renders one display item into a tile; used as a static helper to avoid borrow conflicts.
    fn render_item_to_tile_static(
        item: &DisplayItem,
        tile: &mut super::tile::Tile,
        tile_bounds: &Rect,
    ) -> AureaResult<()> {
        match &item.command {
            super::super::command::DrawCommand::Clear(color) => {
                let rgba = color_to_u32(*color);
                tile.clear(rgba);
            }
            super::super::command::DrawCommand::DrawRect(rect, paint) => {
                Self::draw_rect_to_tile_static(rect, paint, item.blend_mode, tile, tile_bounds);
            }
            super::super::command::DrawCommand::DrawCircle(center, radius, paint) => {
                Self::draw_circle_to_tile_static(
                    *center,
                    *radius,
                    paint,
                    item.blend_mode,
                    tile,
                    tile_bounds,
                );
            }
            super::super::command::DrawCommand::DrawPath(path, paint) => {
                Self::draw_path_to_tile_static(path, paint, item.blend_mode, tile, tile_bounds)?;
            }
            super::super::command::DrawCommand::DrawImageRect(image, dest) => {
                Self::draw_image_to_tile_static(
                    image,
                    Rect::new(0.0, 0.0, image.width as f32, image.height as f32),
                    *dest,
                    item.blend_mode,
                    tile,
                    tile_bounds,
                );
            }
            super::super::command::DrawCommand::DrawImageRegion(image, src, dest) => {
                Self::draw_image_to_tile_static(
                    image,
                    *src,
                    *dest,
                    item.blend_mode,
                    tile,
                    tile_bounds,
                );
            }
            super::super::command::DrawCommand::FillLinearGradient(gradient, rect) => {
                Self::fill_linear_gradient_to_tile_static(
                    gradient,
                    *rect,
                    item.blend_mode,
                    tile,
                    tile_bounds,
                );
            }
            super::super::command::DrawCommand::FillRadialGradient(gradient, rect) => {
                Self::fill_radial_gradient_to_tile_static(
                    gradient,
                    *rect,
                    item.blend_mode,
                    tile,
                    tile_bounds,
                );
            }
            _ => {}
        }
        Ok(())
    }

    /// Renders one display item into a tile (instance wrapper around the static helper).
    #[allow(dead_code)]
    fn render_item_to_tile(
        &self,
        item: &DisplayItem,
        tile: &mut super::tile::Tile,
        tile_bounds: &Rect,
    ) -> AureaResult<()> {
        Self::render_item_to_tile_static(item, tile, tile_bounds)
    }

    fn set_pixel_blend(tile: &mut super::tile::Tile, lx: u32, ly: u32, src: u32, mode: BlendMode) {
        if mode == BlendMode::Normal {
            tile.set_pixel(lx, ly, src);
        } else {
            let dst = tile.get_pixel(lx, ly);
            tile.set_pixel(lx, ly, blend_pixel(src, dst, mode));
        }
    }

    /// Fills or strokes a rectangle within a single tile, clipping to the tile and applying blend.
    fn draw_rect_to_tile_static(
        rect: &Rect,
        paint: &Paint,
        blend_mode: BlendMode,
        tile: &mut super::tile::Tile,
        tile_bounds: &Rect,
    ) {
        let clip_rect = Rect::new(
            rect.x.max(tile_bounds.x),
            rect.y.max(tile_bounds.y),
            (rect.x + rect.width).min(tile_bounds.x + tile_bounds.width)
                - rect.x.max(tile_bounds.x),
            (rect.y + rect.height).min(tile_bounds.y + tile_bounds.height)
                - rect.y.max(tile_bounds.y),
        );

        if clip_rect.width <= 0.0 || clip_rect.height <= 0.0 {
            return;
        }

        match paint.style {
            PaintStyle::Fill => {
                let start_x = clip_rect.x as u32;
                let start_y = clip_rect.y as u32;
                let end_x = (clip_rect.x + clip_rect.width) as u32;
                let end_y = (clip_rect.y + clip_rect.height) as u32;

                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let coverage = rect_coverage(rect, x as f32, y as f32);
                        let color = color_to_u32_with_coverage(paint.color, coverage);
                        let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                        Self::set_pixel_blend(tile, local_x, local_y, color, blend_mode);
                    }
                }
            }
            PaintStyle::Stroke => {
                let color = color_to_u32(paint.color);
                let stroke_width = paint.stroke_width as u32;
                if stroke_width == 0 {
                    return;
                }

                let end_x = (clip_rect.x + clip_rect.width) as u32;
                let end_y = (clip_rect.y + clip_rect.height) as u32;

                for x in (clip_rect.x as u32)..end_x {
                    for y in (clip_rect.y as u32)
                        ..((clip_rect.y + stroke_width as f32) as u32).min(end_y)
                    {
                        let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                        Self::set_pixel_blend(tile, local_x, local_y, color, blend_mode);
                    }
                }

                let bottom_y = (rect.y + rect.height) as u32;
                for x in (clip_rect.x as u32)..((clip_rect.x + clip_rect.width) as u32) {
                    for y in (bottom_y.saturating_sub(stroke_width))..bottom_y {
                        if y >= clip_rect.y as u32 && y < (clip_rect.y + clip_rect.height) as u32 {
                            let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                            Self::set_pixel_blend(tile, local_x, local_y, color, blend_mode);
                        }
                    }
                }

                for y in (clip_rect.y as u32)..((clip_rect.y + clip_rect.height) as u32) {
                    for x in (clip_rect.x as u32)..((clip_rect.x + stroke_width as f32) as u32) {
                        let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                        Self::set_pixel_blend(tile, local_x, local_y, color, blend_mode);
                    }
                }

                let right_x = (rect.x + rect.width) as u32;
                for y in (clip_rect.y as u32)..((clip_rect.y + clip_rect.height) as u32) {
                    for x in (right_x.saturating_sub(stroke_width))..right_x {
                        if x >= clip_rect.x as u32 && x < (clip_rect.x + clip_rect.width) as u32 {
                            let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                            Self::set_pixel_blend(tile, local_x, local_y, color, blend_mode);
                        }
                    }
                }
            }
        }
    }

    /// Fills or strokes a circle within a single tile, clipping to the tile and applying blend.
    fn draw_circle_to_tile_static(
        center: Point,
        radius: f32,
        paint: &Paint,
        blend_mode: BlendMode,
        tile: &mut super::tile::Tile,
        tile_bounds: &Rect,
    ) {
        let r_squared = radius * radius;

        match paint.style {
            PaintStyle::Fill => {
                let start_x = (center.x - radius).max(tile_bounds.x) as u32;
                let start_y = (center.y - radius).max(tile_bounds.y) as u32;
                let end_x = ((center.x + radius).min(tile_bounds.x + tile_bounds.width)) as u32;
                let end_y = ((center.y + radius).min(tile_bounds.y + tile_bounds.height)) as u32;

                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let coverage = circle_coverage(center, radius, x as f32, y as f32);
                        if coverage <= 0.0 {
                            continue;
                        }
                        let color = color_to_u32_with_coverage(paint.color, coverage);
                        let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                        Self::set_pixel_blend(tile, local_x, local_y, color, blend_mode);
                    }
                }
            }
            PaintStyle::Stroke => {
                let color = color_to_u32(paint.color);
                let stroke_width = paint.stroke_width;
                let inner_radius = radius - stroke_width;
                let inner_r_squared = inner_radius * inner_radius;

                let start_x = (center.x - radius).max(tile_bounds.x) as u32;
                let start_y = (center.y - radius).max(tile_bounds.y) as u32;
                let end_x = ((center.x + radius).min(tile_bounds.x + tile_bounds.width)) as u32;
                let end_y = ((center.y + radius).min(tile_bounds.y + tile_bounds.height)) as u32;

                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let dx = x as f32 - center.x;
                        let dy = y as f32 - center.y;
                        let dist_squared = dx * dx + dy * dy;
                        if dist_squared <= r_squared && dist_squared >= inner_r_squared {
                            let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                            Self::set_pixel_blend(tile, local_x, local_y, color, blend_mode);
                        }
                    }
                }
            }
        }
    }

    /// Returns the tile store for direct buffer access.
    pub fn tile_store(&self) -> &TileStore {
        &self.tile_store
    }

    /// Returns mutable access to the tile store.
    pub fn tile_store_mut(&mut self) -> &mut TileStore {
        &mut self.tile_store
    }

    /// Builds a flat RGBA buffer from all tiles for the platform to blit. The buffer is
    /// intentionally leaked; the platform consumes it before the next frame.
    pub fn get_buffer(&self) -> (*const u8, usize, u32, u32) {
        let buffer_size = (self.width * self.height) as usize;
        let mut buffer = vec![0u32; buffer_size];
        self.tile_store
            .copy_to_buffer(&mut buffer, self.width, self.height);
        let leaked = Box::leak(Box::new(buffer));
        (
            leaked.as_ptr() as *const u8,
            leaked.len() * 4,
            self.width,
            self.height,
        )
    }

    /// Returns the current display list (used for hit testing and interaction).
    pub fn display_list(&self) -> &DisplayList {
        &self.display_list
    }

    /// Fills or strokes a path within a single tile using scanlines, clipping to the tile and applying blend.
    fn draw_path_to_tile_static(
        path: &super::super::types::Path,
        paint: &Paint,
        blend_mode: BlendMode,
        tile: &mut super::tile::Tile,
        tile_bounds: &Rect,
    ) -> AureaResult<()> {
        let edges = tessellate_path(path);

        if edges.is_empty() {
            return Ok(());
        }

        let mut y_min = f32::MAX;
        let mut y_max = f32::MIN;
        for edge in &edges {
            y_min = y_min.min(edge.y_min);
            y_max = y_max.max(edge.y_max);
        }

        let y_start = y_min.max(tile_bounds.y).ceil() as u32;
        let y_end = y_max.min(tile_bounds.y + tile_bounds.height).ceil() as u32;

        let tile_pixels = tile.pixels_mut();
        let tile_width = TILE_SIZE;
        let tile_height = TILE_SIZE;
        let tile_offset_x = (tile_bounds.x as u32) / TILE_SIZE * TILE_SIZE;
        let tile_offset_y = (tile_bounds.y as u32) / TILE_SIZE * TILE_SIZE;

        match paint.style {
            PaintStyle::Fill => {
                for y in y_start..y_end {
                    fill_scanline(
                        &edges,
                        y as f32,
                        tile_pixels,
                        tile_width,
                        tile_height,
                        tile_offset_x,
                        tile_offset_y,
                        paint.color,
                        blend_mode,
                    );
                }
            }
            PaintStyle::Stroke => {
                for y in y_start..y_end {
                    fill_scanline(
                        &edges,
                        y as f32,
                        tile_pixels,
                        tile_width,
                        tile_height,
                        tile_offset_x,
                        tile_offset_y,
                        paint.color,
                        blend_mode,
                    );
                }
            }
        }

        Ok(())
    }

    fn draw_image_to_tile_static(
        image: &Image,
        src: Rect,
        dest: Rect,
        blend_mode: BlendMode,
        tile: &mut super::tile::Tile,
        tile_bounds: &Rect,
    ) {
        if image.data.is_empty()
            || image.width == 0
            || image.height == 0
            || dest.width <= 0.0
            || dest.height <= 0.0
        {
            return;
        }
        let clip_left = dest.x.max(tile_bounds.x);
        let clip_top = dest.y.max(tile_bounds.y);
        let clip_right = (dest.x + dest.width).min(tile_bounds.x + tile_bounds.width);
        let clip_bottom = (dest.y + dest.height).min(tile_bounds.y + tile_bounds.height);
        if clip_left >= clip_right || clip_top >= clip_bottom {
            return;
        }
        let tile_origin_x = tile_bounds.x;
        let tile_origin_y = tile_bounds.y;
        let start_x = clip_left.ceil() as i32;
        let end_x = clip_right.floor() as i32;
        let start_y = clip_top.ceil() as i32;
        let end_y = clip_bottom.floor() as i32;
        for cy in start_y..end_y {
            for cx in start_x..end_x {
                let u = (cx as f32 - dest.x) / dest.width * src.width + src.x;
                let v = (cy as f32 - dest.y) / dest.height * src.height + src.y;
                let sx = u.clamp(0.0, image.width as f32 - 0.001) as u32;
                let sy = v.clamp(0.0, image.height as f32 - 0.001) as u32;
                let idx = (sy as usize * image.width as usize + sx as usize) * 4;
                if idx + 3 >= image.data.len() {
                    continue;
                }
                let r = image.data[idx];
                let g = image.data[idx + 1];
                let b = image.data[idx + 2];
                let a = image.data[idx + 3];
                let lx = (cx as f32 - tile_origin_x) as u32;
                let ly = (cy as f32 - tile_origin_y) as u32;
                if lx >= TILE_SIZE || ly >= TILE_SIZE {
                    continue;
                }
                let src_rgba =
                    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                Self::set_pixel_blend(tile, lx, ly, src_rgba, blend_mode);
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
        for i in 0..stops.len() - 1 {
            let a = stops[i].offset;
            let b = stops[i + 1].offset;
            if t >= a && t <= b {
                let denom = b - a;
                let s = if denom.abs() < 1e-6 {
                    1.0
                } else {
                    (t - a) / denom
                };
                let c0 = stops[i].color;
                let c1 = stops[i + 1].color;
                let r = (c0.r as f32 + (c1.r as f32 - c0.r as f32) * s).round() as u8;
                let g = (c0.g as f32 + (c1.g as f32 - c0.g as f32) * s).round() as u8;
                let b = (c0.b as f32 + (c1.b as f32 - c0.b as f32) * s).round() as u8;
                let a = (c0.a as f32 + (c1.a as f32 - c0.a as f32) * s).round() as u8;
                return ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            }
        }
        let c = if t <= stops[0].offset {
            stops[0].color
        } else {
            stops.last().map(|s| s.color).unwrap_or(stops[0].color)
        };
        ((c.a as u32) << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
    }

    fn fill_linear_gradient_to_tile_static(
        gradient: &LinearGradient,
        rect: Rect,
        blend_mode: BlendMode,
        tile: &mut super::tile::Tile,
        tile_bounds: &Rect,
    ) {
        let dx = gradient.end.x - gradient.start.x;
        let dy = gradient.end.y - gradient.start.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq < 1e-10 {
            return;
        }
        let clip_left = rect.x.max(tile_bounds.x);
        let clip_top = rect.y.max(tile_bounds.y);
        let clip_right = (rect.x + rect.width).min(tile_bounds.x + tile_bounds.width);
        let clip_bottom = (rect.y + rect.height).min(tile_bounds.y + tile_bounds.height);
        if clip_left >= clip_right || clip_top >= clip_bottom {
            return;
        }
        let tile_origin_x = tile_bounds.x;
        let tile_origin_y = tile_bounds.y;
        let start_x = clip_left.ceil() as i32;
        let end_x = clip_right.floor() as i32;
        let start_y = clip_top.ceil() as i32;
        let end_y = clip_bottom.floor() as i32;
        for cy in start_y..end_y {
            for cx in start_x..end_x {
                let px_f = cx as f32 + 0.5;
                let py_f = cy as f32 + 0.5;
                let t = ((px_f - gradient.start.x) * dx + (py_f - gradient.start.y) * dy) / len_sq;
                let t = t.clamp(0.0, 1.0);
                let rgba = Self::gradient_color_at(&gradient.stops, t);
                let lx = (cx as f32 - tile_origin_x) as u32;
                let ly = (cy as f32 - tile_origin_y) as u32;
                if lx < TILE_SIZE && ly < TILE_SIZE {
                    Self::set_pixel_blend(tile, lx, ly, rgba, blend_mode);
                }
            }
        }
    }

    fn fill_radial_gradient_to_tile_static(
        gradient: &RadialGradient,
        rect: Rect,
        blend_mode: BlendMode,
        tile: &mut super::tile::Tile,
        tile_bounds: &Rect,
    ) {
        if gradient.radius <= 0.0 {
            return;
        }
        let clip_left = rect.x.max(tile_bounds.x);
        let clip_top = rect.y.max(tile_bounds.y);
        let clip_right = (rect.x + rect.width).min(tile_bounds.x + tile_bounds.width);
        let clip_bottom = (rect.y + rect.height).min(tile_bounds.y + tile_bounds.height);
        if clip_left >= clip_right || clip_top >= clip_bottom {
            return;
        }
        let tile_origin_x = tile_bounds.x;
        let tile_origin_y = tile_bounds.y;
        let start_x = clip_left.ceil() as i32;
        let end_x = clip_right.floor() as i32;
        let start_y = clip_top.ceil() as i32;
        let end_y = clip_bottom.floor() as i32;
        for cy in start_y..end_y {
            for cx in start_x..end_x {
                let px_f = cx as f32 + 0.5;
                let py_f = cy as f32 + 0.5;
                let dist = ((px_f - gradient.center.x).powi(2)
                    + (py_f - gradient.center.y).powi(2))
                .sqrt();
                let t = (dist / gradient.radius).min(1.0);
                let rgba = Self::gradient_color_at(&gradient.stops, t);
                let lx = (cx as f32 - tile_origin_x) as u32;
                let ly = (cy as f32 - tile_origin_y) as u32;
                if lx < TILE_SIZE && ly < TILE_SIZE {
                    Self::set_pixel_blend(tile, lx, ly, rgba, blend_mode);
                }
            }
        }
    }
}

impl Renderer for CpuRasterizer {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.width = info.width;
        self.height = info.height;
        self.scale_factor = info.scale_factor;
        self.tile_store = TileStore::new(info.width, info.height);
        self.initialized = true;
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.width = width;
        self.height = height;
        self.tile_store.resize(width, height);
        self.display_list.clear();
        Ok(())
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        self.display_list.clear();
        let mut ctx = CpuDrawingContext::new(
            &mut self.display_list as *mut DisplayList,
            self.width,
            self.height,
        );
        ctx.set_scale_factor(self.scale_factor);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        let damage = self
            .pending_damage
            .take()
            .unwrap_or_else(|| Rect::new(0.0, 0.0, self.width as f32, self.height as f32));

        let display_items: Vec<_> = self.display_list.items().to_vec();
        self.tile_store.mark_damaged(&damage);
        let dirty_tiles: Vec<_> = self.tile_store.dirty_tiles();

        let mut background_color = 0u32;
        for item in &display_items {
            if let super::super::command::DrawCommand::Clear(color) = &item.command {
                background_color = color_to_u32(*color);
                break;
            }
        }

        for (tile_x, tile_y) in dirty_tiles {
            let tile_bounds = Rect::new(
                (tile_x * TILE_SIZE) as f32,
                (tile_y * TILE_SIZE) as f32,
                TILE_SIZE as f32,
                TILE_SIZE as f32,
            );

            if let Some(tile) = self.tile_store.get_tile_mut(tile_x, tile_y) {
                tile.clear(background_color);
                for item in &display_items {
                    if item.intersects(&tile_bounds) {
                        Self::render_item_to_tile_static(item, tile, &tile_bounds)?;
                    }
                }

                tile.mark_clean();
            }
        }

        use crate::render::renderer::CURRENT_BUFFER;
        let (ptr, size, width, height) = self.get_buffer();
        CURRENT_BUFFER.with(|buf| {
            *buf.borrow_mut() = Some((ptr, size, width, height));
        });

        Ok(())
    }

    fn cleanup(&mut self) {
        self.initialized = false;
        self.display_list.clear();
        self.pending_damage = None;
    }

    fn set_damage(&mut self, damage: Option<Rect>) {
        self.pending_damage = damage;
    }

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }
}

/// Packs a Color into a single u32 (A in high byte, then R, G, B).
fn color_to_u32(color: Color) -> u32 {
    ((color.a as u32) << 24) | ((color.r as u32) << 16) | ((color.g as u32) << 8) | (color.b as u32)
}

/// Packs a Color with alpha scaled by coverage (0.0..=1.0) for anti-aliasing.
fn color_to_u32_with_coverage(color: Color, coverage: f32) -> u32 {
    let a = ((color.a as f32 * coverage).round().clamp(0.0, 255.0)) as u32;
    (a << 24) | ((color.r as u32) << 16) | ((color.g as u32) << 8) | (color.b as u32)
}

/// Fraction of the pixel [px, px+1) x [py, py+1) that lies inside the rect.
fn rect_coverage(rect: &Rect, px: f32, py: f32) -> f32 {
    let left = px.max(rect.x);
    let right = (px + 1.0).min(rect.x + rect.width);
    let top = py.max(rect.y);
    let bottom = (py + 1.0).min(rect.y + rect.height);
    if left >= right || top >= bottom {
        0.0
    } else {
        (right - left) * (bottom - top)
    }
}

/// Approximate coverage of the pixel with center (px+0.5, py+0.5) inside the circle; 1.0 inside, 0.0 outside, linear ramp in the one-pixel band.
fn circle_coverage(center: Point, radius: f32, px: f32, py: f32) -> f32 {
    let cx = px + 0.5;
    let cy = py + 0.5;
    let dx = cx - center.x;
    let dy = cy - center.y;
    let d = (dx * dx + dy * dy).sqrt();
    if d <= radius - 0.5 {
        1.0
    } else if d >= radius + 0.5 {
        0.0
    } else {
        (radius + 0.5 - d).clamp(0.0, 1.0)
    }
}
