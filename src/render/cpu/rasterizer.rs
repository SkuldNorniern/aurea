//! CPU rasterizer for tile-based rendering
//!
//! This rasterizer processes display items and renders them to tiles,
//! enabling efficient partial redraw of only damaged regions.

use crate::AureaResult;
use super::tile::{TileStore, TILE_SIZE};
use super::cache::BoundedCache;
use super::context::CpuDrawingContext;
use super::super::types::{Color, Rect, Point, Paint, PaintStyle};
use super::super::display_list::{DisplayList, DisplayItem};
use super::super::renderer::{Renderer, DrawingContext};
use super::super::surface::{Surface, SurfaceInfo};

/// CPU rasterizer with tile-based backing store
pub struct CpuRasterizer {
    tile_store: TileStore,
    cache: BoundedCache<Vec<u32>>,
    width: u32,
    height: u32,
    display_list: DisplayList,
    initialized: bool,
    scale_factor: f32,
}

impl CpuRasterizer {
    /// Create a new CPU rasterizer
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            tile_store: TileStore::new(width, height),
            cache: BoundedCache::new(16 * 1024 * 1024), // 16MB cache budget
            width,
            height,
            display_list: DisplayList::new(),
            initialized: false,
            scale_factor: 1.0,
        }
    }
    
    /// Resize the rasterizer
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.tile_store.resize(width, height);
    }
    
    /// Render display items to tiles, only updating damaged regions
    pub fn render(&mut self, display_list: &DisplayList, damage: &Rect) -> AureaResult<()> {
        // Mark damaged tiles
        self.tile_store.mark_damaged(damage);
        
        // Collect dirty tiles first to avoid borrowing issues
        let dirty_tiles: Vec<_> = self.tile_store.dirty_tiles();
        
        // Render only dirty tiles
        for (tile_x, tile_y) in dirty_tiles {
            self.render_tile(tile_x, tile_y, display_list)?;
        }
        
        Ok(())
    }
    
    /// Render a single tile
    fn render_tile(&mut self, tile_x: u32, tile_y: u32, display_list: &DisplayList) -> AureaResult<()> {
        let tile_bounds = Rect::new(
            (tile_x * TILE_SIZE) as f32,
            (tile_y * TILE_SIZE) as f32,
            TILE_SIZE as f32,
            TILE_SIZE as f32,
        );
        
        // Collect items that intersect this tile first
        let intersecting_items: Vec<_> = display_list.items()
            .iter()
            .filter(|item| item.intersects(&tile_bounds))
            .collect();
        
        if let Some(tile) = self.tile_store.get_tile_mut(tile_x, tile_y) {
            // Clear tile with background color (or transparent)
            tile.clear(0);
            
            // Render items that intersect this tile
            // Note: We don't need tile_store in the static method, so we pass a dummy reference
            // The pixel_to_local calculation is done inline
            for item in intersecting_items {
                Self::render_item_to_tile_static(item, tile, &tile_bounds)?;
            }
            
            tile.mark_clean();
        }
        
        Ok(())
    }
    
    /// Static helper to render item to tile (avoids borrowing issues)
    fn render_item_to_tile_static(
        item: &DisplayItem,
        tile: &mut super::tile::Tile,
        tile_bounds: &Rect,
    ) -> AureaResult<()> {
        match &item.command {
            super::super::renderer::DrawCommand::Clear(color) => {
                let rgba = color_to_u32(*color);
                tile.clear(rgba);
            }
            super::super::renderer::DrawCommand::DrawRect(rect, paint) => {
                Self::draw_rect_to_tile_static(rect, paint, tile, tile_bounds);
            }
            super::super::renderer::DrawCommand::DrawCircle(center, radius, paint) => {
                Self::draw_circle_to_tile_static(*center, *radius, paint, tile, tile_bounds);
            }
            _ => {
                // Other commands not yet implemented
            }
        }
        Ok(())
    }
    
    /// Render a display item to a tile (instance method for compatibility)
    fn render_item_to_tile(&self, item: &DisplayItem, tile: &mut super::tile::Tile, tile_bounds: &Rect) -> AureaResult<()> {
        Self::render_item_to_tile_static(item, tile, tile_bounds)
    }
    
    /// Draw a rectangle to a tile (static helper)
    fn draw_rect_to_tile_static(rect: &Rect, paint: &Paint, tile: &mut super::tile::Tile, tile_bounds: &Rect) {
        // Calculate intersection of rect with tile
        let clip_rect = Rect::new(
            rect.x.max(tile_bounds.x),
            rect.y.max(tile_bounds.y),
            (rect.x + rect.width).min(tile_bounds.x + tile_bounds.width) - rect.x.max(tile_bounds.x),
            (rect.y + rect.height).min(tile_bounds.y + tile_bounds.height) - rect.y.max(tile_bounds.y),
        );
        
        if clip_rect.width <= 0.0 || clip_rect.height <= 0.0 {
            return;
        }
        
        let color = color_to_u32(paint.color);
        
        match paint.style {
            PaintStyle::Fill => {
                let start_x = clip_rect.x as u32;
                let start_y = clip_rect.y as u32;
                let end_x = (clip_rect.x + clip_rect.width) as u32;
                let end_y = (clip_rect.y + clip_rect.height) as u32;
                
                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                        tile.set_pixel(local_x, local_y, color);
                    }
                }
            }
            PaintStyle::Stroke => {
                let stroke_width = paint.stroke_width as u32;
                if stroke_width == 0 {
                    return;
                }
                
                let end_x = (clip_rect.x + clip_rect.width) as u32;
                let end_y = (clip_rect.y + clip_rect.height) as u32;
                
                // Draw top edge
                for x in (clip_rect.x as u32)..end_x {
                    for y in (clip_rect.y as u32)..((clip_rect.y + stroke_width as f32) as u32).min(end_y) {
                        let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                        tile.set_pixel(local_x, local_y, color);
                    }
                }
                
                // Draw bottom edge
                let bottom_y = (rect.y + rect.height) as u32;
                for x in (clip_rect.x as u32)..((clip_rect.x + clip_rect.width) as u32) {
                    for y in (bottom_y.saturating_sub(stroke_width))..bottom_y {
                        if y >= clip_rect.y as u32 && y < (clip_rect.y + clip_rect.height) as u32 {
                            let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                            tile.set_pixel(local_x, local_y, color);
                        }
                    }
                }
                
                // Draw left edge
                for y in (clip_rect.y as u32)..((clip_rect.y + clip_rect.height) as u32) {
                    for x in (clip_rect.x as u32)..((clip_rect.x + stroke_width as f32) as u32) {
                        let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                        tile.set_pixel(local_x, local_y, color);
                    }
                }
                
                // Draw right edge
                let right_x = (rect.x + rect.width) as u32;
                for y in (clip_rect.y as u32)..((clip_rect.y + clip_rect.height) as u32) {
                    for x in (right_x.saturating_sub(stroke_width))..right_x {
                        if x >= clip_rect.x as u32 && x < (clip_rect.x + clip_rect.width) as u32 {
                            let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                            tile.set_pixel(local_x, local_y, color);
                        }
                    }
                }
            }
        }
    }
    
    /// Draw a circle to a tile (static helper)
    fn draw_circle_to_tile_static(center: Point, radius: f32, paint: &Paint, tile: &mut super::tile::Tile, tile_bounds: &Rect) {
        let color = color_to_u32(paint.color);
        let r_squared = radius * radius;
        
        match paint.style {
            PaintStyle::Fill => {
                let start_x = (center.x - radius).max(tile_bounds.x) as u32;
                let start_y = (center.y - radius).max(tile_bounds.y) as u32;
                let end_x = ((center.x + radius).min(tile_bounds.x + tile_bounds.width)) as u32;
                let end_y = ((center.y + radius).min(tile_bounds.y + tile_bounds.height)) as u32;
                
                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let dx = x as f32 - center.x;
                        let dy = y as f32 - center.y;
                        if dx * dx + dy * dy <= r_squared {
                            let (local_x, local_y) = (x % TILE_SIZE, y % TILE_SIZE);
                            tile.set_pixel(local_x, local_y, color);
                        }
                    }
                }
            }
            PaintStyle::Stroke => {
                // Simple circle outline
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
                            tile.set_pixel(local_x, local_y, color);
                        }
                    }
                }
            }
        }
    }
    
    /// Get the tile store (for buffer access)
    pub fn tile_store(&self) -> &TileStore {
        &self.tile_store
    }
    
    /// Get mutable tile store
    pub fn tile_store_mut(&mut self) -> &mut TileStore {
        &mut self.tile_store
    }
    
    /// Get the render buffer (flat RGBA buffer for platform blitting)
    /// Note: This creates a temporary buffer. For production, we'd want to cache this.
    pub fn get_buffer(&self) -> (*const u8, usize, u32, u32) {
        let buffer_size = (self.width * self.height) as usize;
        let mut buffer = vec![0u32; buffer_size];
        self.tile_store.copy_to_buffer(&mut buffer, self.width, self.height);
        // Leak the buffer - it will be freed when the frame ends
        let leaked = Box::leak(Box::new(buffer));
        (leaked.as_ptr() as *const u8, leaked.len() * 4, self.width, self.height)
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
        // Clear display list on resize
        self.display_list.clear();
        Ok(())
    }
    
    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        // Clear the display list for this frame
        self.display_list.clear();
        let mut ctx = CpuDrawingContext::new(&mut self.display_list as *mut DisplayList);
        ctx.set_scale_factor(self.scale_factor);
        Ok(Box::new(ctx))
    }
    
    fn end_frame(&mut self) -> AureaResult<()> {
        // For now, we'll render the full canvas as damage
        // TODO: Integrate with actual damage tracking from Window
        let damage = Rect::new(0.0, 0.0, self.width as f32, self.height as f32);
        
        // Collect display list items first to avoid borrowing issues
        let display_items: Vec<_> = self.display_list.items().to_vec();
        
        // Mark damaged tiles and render (inline to avoid borrowing issues)
        self.tile_store.mark_damaged(&damage);
        let dirty_tiles: Vec<_> = self.tile_store.dirty_tiles();
        
        for (tile_x, tile_y) in dirty_tiles {
            let tile_bounds = Rect::new(
                (tile_x * TILE_SIZE) as f32,
                (tile_y * TILE_SIZE) as f32,
                TILE_SIZE as f32,
                TILE_SIZE as f32,
            );
            
            if let Some(tile) = self.tile_store.get_tile_mut(tile_x, tile_y) {
                tile.clear(0);
                
                for item in &display_items {
                    if item.intersects(&tile_bounds) {
                        Self::render_item_to_tile_static(item, tile, &tile_bounds)?;
                    }
                }
                
                tile.mark_clean();
            }
        }
        
        // Update the thread-local buffer for platform blitting
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
    }
}

/// Convert Color to u32 RGBA
fn color_to_u32(color: Color) -> u32 {
    ((color.a as u32) << 24)
        | ((color.r as u32) << 16)
        | ((color.g as u32) << 8)
        | (color.b as u32)
}

