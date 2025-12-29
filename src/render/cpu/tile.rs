//! Tile-based backing store for efficient partial redraw
//!
//! Tiles are fixed-size rectangular regions of the canvas. Only tiles that
//! intersect with the damage region need to be redrawn, enabling efficient
//! incremental updates.

use super::super::types::Rect;
use crate::AureaResult;

/// Tile size in pixels (64x64 is a good balance for most UI)
pub const TILE_SIZE: u32 = 64;

/// A single tile in the backing store
#[derive(Debug, Clone)]
pub struct Tile {
    /// Tile X coordinate (in tile units, not pixels)
    pub tile_x: u32,
    /// Tile Y coordinate (in tile units, not pixels)
    pub tile_y: u32,
    /// Pixel data (RGBA, row-major order)
    pub pixels: Vec<u32>,
    /// Whether this tile is dirty and needs redraw
    pub dirty: bool,
}

impl Tile {
    /// Create a new tile
    pub fn new(tile_x: u32, tile_y: u32) -> Self {
        Self {
            tile_x,
            tile_y,
            pixels: vec![0; (TILE_SIZE * TILE_SIZE) as usize],
            dirty: false,
        }
    }
    
    /// Get the pixel bounds of this tile in canvas coordinates
    pub fn bounds(&self) -> Rect {
        Rect::new(
            (self.tile_x * TILE_SIZE) as f32,
            (self.tile_y * TILE_SIZE) as f32,
            TILE_SIZE as f32,
            TILE_SIZE as f32,
        )
    }
    
    /// Mark this tile as dirty
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
    
    /// Mark this tile as clean
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }
    
    /// Set a pixel in this tile (local coordinates, 0..TILE_SIZE)
    pub fn set_pixel(&mut self, x: u32, y: u32, color: u32) {
        if x < TILE_SIZE && y < TILE_SIZE {
            let index = (y * TILE_SIZE + x) as usize;
            if index < self.pixels.len() {
                self.pixels[index] = color;
            }
        }
    }
    
    /// Get a pixel from this tile (local coordinates)
    pub fn get_pixel(&self, x: u32, y: u32) -> u32 {
        if x < TILE_SIZE && y < TILE_SIZE {
            let index = (y * TILE_SIZE + x) as usize;
            if index < self.pixels.len() {
                self.pixels[index]
            } else {
                0
            }
        } else {
            0
        }
    }
    
    /// Clear the tile with a color
    pub fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
    }
    
    /// Get mutable reference to pixel buffer
    pub fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.pixels
    }
}

/// Tile-based backing store for a canvas
pub struct TileStore {
    /// Canvas width in pixels
    width: u32,
    /// Canvas height in pixels
    height: u32,
    /// Tiles organized by (tile_y, tile_x)
    tiles: Vec<Vec<Tile>>,
    /// Number of tiles horizontally
    tile_width: u32,
    /// Number of tiles vertically
    tile_height: u32,
}

impl TileStore {
    /// Create a new tile store for a canvas of given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        let tile_width = (width + TILE_SIZE - 1) / TILE_SIZE;
        let tile_height = (height + TILE_SIZE - 1) / TILE_SIZE;
        
        let mut tiles = Vec::new();
        for y in 0..tile_height {
            let mut row = Vec::new();
            for x in 0..tile_width {
                row.push(Tile::new(x, y));
            }
            tiles.push(row);
        }
        
        Self {
            width,
            height,
            tiles,
            tile_width,
            tile_height,
        }
    }
    
    /// Resize the tile store
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.tile_width = (width + TILE_SIZE - 1) / TILE_SIZE;
        self.tile_height = (height + TILE_SIZE - 1) / TILE_SIZE;
        
        self.tiles.clear();
        for y in 0..self.tile_height {
            let mut row = Vec::new();
            for x in 0..self.tile_width {
                row.push(Tile::new(x, y));
            }
            self.tiles.push(row);
        }
    }
    
    /// Get canvas dimensions
    pub fn width(&self) -> u32 {
        self.width
    }
    
    pub fn height(&self) -> u32 {
        self.height
    }
    
    /// Get a tile by tile coordinates
    pub fn get_tile(&self, tile_x: u32, tile_y: u32) -> Option<&Tile> {
        if tile_y < self.tile_height && tile_x < self.tile_width {
            self.tiles.get(tile_y as usize)
                .and_then(|row| row.get(tile_x as usize))
        } else {
            None
        }
    }
    
    /// Get a mutable tile by tile coordinates
    pub fn get_tile_mut(&mut self, tile_x: u32, tile_y: u32) -> Option<&mut Tile> {
        if tile_y < self.tile_height && tile_x < self.tile_width {
            self.tiles.get_mut(tile_y as usize)
                .and_then(|row| row.get_mut(tile_x as usize))
        } else {
            None
        }
    }
    
    /// Mark tiles intersecting a damage region as dirty
    pub fn mark_damaged(&mut self, damage: &Rect) {
        let start_tile_x = (damage.x as u32) / TILE_SIZE;
        let start_tile_y = (damage.y as u32) / TILE_SIZE;
        let end_tile_x = ((damage.x + damage.width) as u32 + TILE_SIZE - 1) / TILE_SIZE;
        let end_tile_y = ((damage.y + damage.height) as u32 + TILE_SIZE - 1) / TILE_SIZE;
        
        for y in start_tile_y..end_tile_y.min(self.tile_height) {
            for x in start_tile_x..end_tile_x.min(self.tile_width) {
                if let Some(tile) = self.get_tile_mut(x, y) {
                    tile.mark_dirty();
                }
            }
        }
    }
    
    /// Get all dirty tiles
    pub fn dirty_tiles(&self) -> Vec<(u32, u32)> {
        let mut result = Vec::new();
        for y in 0..self.tile_height {
            for x in 0..self.tile_width {
                if let Some(tile) = self.get_tile(x, y) {
                    if tile.dirty {
                        result.push((x, y));
                    }
                }
            }
        }
        result
    }
    
    /// Mark all tiles as clean
    pub fn mark_all_clean(&mut self) {
        for row in &mut self.tiles {
            for tile in row {
                tile.mark_clean();
            }
        }
    }
    
    /// Get tile coordinates for a pixel position
    pub fn pixel_to_tile(&self, x: u32, y: u32) -> (u32, u32) {
        (x / TILE_SIZE, y / TILE_SIZE)
    }
    
    /// Get local pixel coordinates within a tile
    pub fn pixel_to_local(&self, x: u32, y: u32) -> (u32, u32) {
        (x % TILE_SIZE, y % TILE_SIZE)
    }
    
    /// Copy tile data to a flat buffer (for platform blitting)
    pub fn copy_to_buffer(&self, buffer: &mut [u32], buffer_width: u32, buffer_height: u32) {
        for y in 0..buffer_height.min(self.height) {
            for x in 0..buffer_width.min(self.width) {
                let (tile_x, tile_y) = self.pixel_to_tile(x, y);
                let (local_x, local_y) = self.pixel_to_local(x, y);
                
                if let Some(tile) = self.get_tile(tile_x, tile_y) {
                    let pixel = tile.get_pixel(local_x, local_y);
                    let index = (y * buffer_width + x) as usize;
                    if index < buffer.len() {
                        buffer[index] = pixel;
                    }
                }
            }
        }
    }
    
    /// Copy only dirty tiles to a flat buffer
    pub fn copy_dirty_to_buffer(&self, buffer: &mut [u32], buffer_width: u32, buffer_height: u32) {
        for (tile_x, tile_y) in self.dirty_tiles() {
            let tile_bounds = Rect::new(
                (tile_x * TILE_SIZE) as f32,
                (tile_y * TILE_SIZE) as f32,
                TILE_SIZE as f32,
                TILE_SIZE as f32,
            );
            
            let start_x = tile_bounds.x as u32;
            let start_y = tile_bounds.y as u32;
            let end_x = (start_x + TILE_SIZE).min(self.width);
            let end_y = (start_y + TILE_SIZE).min(self.height);
            
            if let Some(tile) = self.get_tile(tile_x, tile_y) {
                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let (local_x, local_y) = self.pixel_to_local(x, y);
                        let pixel = tile.get_pixel(local_x, local_y);
                        let index = (y * buffer_width + x) as usize;
                        if index < buffer.len() {
                            buffer[index] = pixel;
                        }
                    }
                }
            }
        }
    }
}

