//! Renderer trait for different rendering backends

use std::cell::RefCell;
use crate::AureaResult;
use super::surface::{Surface, SurfaceInfo};
use super::types::{
    Color, Rect, Point, Paint, Path, Transform, Font, TextMetrics,
    LinearGradient, RadialGradient, BlendMode, Image, PaintFill,
};
use log::{debug, info, warn, error, trace};

thread_local! {
    static COMMAND_BUFFER: RefCell<Option<*mut Vec<DrawCommand>>> = RefCell::new(None);
    pub(crate) static CURRENT_BUFFER: RefCell<Option<(*const u8, usize, u32, u32)>> = RefCell::new(None);
}

/// Drawing context for rendering operations
pub trait DrawingContext {
    /// Clear the canvas with a color
    fn clear(&mut self, color: Color) -> AureaResult<()>;

    /// Draw a rectangle
    fn draw_rect(&mut self, rect: Rect, paint: &Paint) -> AureaResult<()>;

    /// Draw a circle
    fn draw_circle(&mut self, center: Point, radius: f32, paint: &Paint) -> AureaResult<()>;

    /// Draw text at a position
    fn draw_text(&mut self, text: &str, position: Point, paint: &Paint) -> AureaResult<()>;

    /// Draw a path
    fn draw_path(&mut self, path: &Path, paint: &Paint) -> AureaResult<()>;

    /// Draw an image at a position
    fn draw_image(&mut self, image: &Image, position: Point) -> AureaResult<()>;

    /// Draw an image with scaling
    fn draw_image_rect(&mut self, image: &Image, dest: Rect) -> AureaResult<()>;

    /// Draw part of an image (source rect) to a destination rect
    fn draw_image_region(&mut self, image: &Image, src: Rect, dest: Rect) -> AureaResult<()>;

    /// Draw text with font configuration
    fn draw_text_with_font(&mut self, text: &str, position: Point, font: &Font, paint: &Paint) -> AureaResult<()>;

    /// Measure text dimensions
    fn measure_text(&mut self, text: &str, font: &Font) -> AureaResult<TextMetrics>;

    /// Save the current transformation matrix
    fn save(&mut self) -> AureaResult<()>;

    /// Restore the previously saved transformation matrix
    fn restore(&mut self) -> AureaResult<()>;

    /// Apply a transformation matrix
    fn transform(&mut self, transform: Transform) -> AureaResult<()>;

    /// Translate the coordinate system
    fn translate(&mut self, x: f32, y: f32) -> AureaResult<()> {
        self.transform(Transform::translate(x, y))
    }

    /// Scale the coordinate system
    fn scale(&mut self, sx: f32, sy: f32) -> AureaResult<()> {
        self.transform(Transform::scale(sx, sy))
    }

    /// Rotate the coordinate system (angle in radians)
    fn rotate(&mut self, angle: f32) -> AureaResult<()> {
        self.transform(Transform::rotate(angle))
    }

    /// Set a clipping rectangle
    fn clip_rect(&mut self, rect: Rect) -> AureaResult<()>;

    /// Set a clipping path
    fn clip_path(&mut self, path: &Path) -> AureaResult<()>;

    /// Set the global alpha (opacity)
    fn set_alpha(&mut self, alpha: f32) -> AureaResult<()>;

    /// Set the blend mode
    fn set_blend_mode(&mut self, mode: BlendMode) -> AureaResult<()>;

    /// Fill with a linear gradient
    fn fill_linear_gradient(&mut self, gradient: &LinearGradient, rect: Rect) -> AureaResult<()>;

    /// Fill with a radial gradient
    fn fill_radial_gradient(&mut self, gradient: &RadialGradient, rect: Rect) -> AureaResult<()>;

    /// Check if a point is inside a path (hit testing)
    fn hit_test_path(&mut self, path: &Path, point: Point) -> AureaResult<bool>;

    /// Check if a point is inside a rectangle (hit testing)
    fn hit_test_rect(&mut self, rect: Rect, point: Point) -> bool {
        point.x >= rect.x
            && point.x <= rect.x + rect.width
            && point.y >= rect.y
            && point.y <= rect.y + rect.height
    }
}

/// Renderer trait for different backends
pub trait Renderer: Send + Sync {
    /// Initialize the renderer with a native surface
    fn init(&mut self, surface: Surface, info: SurfaceInfo) -> AureaResult<()>;

    /// Resize the rendering surface
    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()>;

    /// Begin a new frame and get a drawing context
    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>>;

    /// End the current frame and present
    fn end_frame(&mut self) -> AureaResult<()>;

    /// Cleanup resources
    fn cleanup(&mut self);
}

/// Drawing command for the placeholder renderer
#[derive(Debug, Clone)]
enum DrawCommand {
    Clear(Color),
    DrawRect(Rect, Paint),
    DrawCircle(Point, f32, Paint),
    DrawPath(Path, Paint),
    DrawText(String, Point, Paint),
    DrawTextWithFont(String, Point, Font, Paint),
}

/// Placeholder renderer implementation
/// This will be replaced with actual Skia/Vello implementations
/// For now, this provides a basic software renderer that draws to a buffer
pub struct PlaceholderRenderer {
    initialized: bool,
    width: u32,
    height: u32,
    buffer: Vec<u32>, // RGBA buffer
    commands: Vec<DrawCommand>, // Drawing commands buffer
}

impl PlaceholderRenderer {
    pub fn new() -> Self {
        Self {
            initialized: false,
            width: 0,
            height: 0,
            buffer: Vec::new(),
            commands: Vec::new(),
        }
    }

    /// Get the rendered buffer as a raw pointer
    /// The buffer is in RGBA format, 4 bytes per pixel (u32 values, but we return as u8 pointer)
    pub fn get_buffer(&self) -> (*const u8, usize) {
        if self.buffer.is_empty() {
            return (std::ptr::null(), 0);
        }
        // Convert u32 buffer to u8 pointer (same memory, just different type)
        (self.buffer.as_ptr() as *const u8, self.buffer.len() * std::mem::size_of::<u32>())
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn apply_commands(&mut self) {
        
        let commands = std::mem::take(&mut self.commands);
        let command_count = commands.len();
        trace!("apply_commands: processing {} commands", command_count);
        
        for (i, cmd) in commands.into_iter().enumerate() {
            trace!("Processing command {}: {:?}", i, std::mem::discriminant(&cmd));
            match cmd {
                DrawCommand::Clear(color) => {
                    debug!("Clear command: rgba({}, {}, {}, {})", color.r, color.g, color.b, color.a);
                    let rgba = ((color.a as u32) << 24)
                        | ((color.r as u32) << 16)
                        | ((color.g as u32) << 8)
                        | (color.b as u32);
                    self.buffer.fill(rgba);
                    debug!("Buffer filled with clear color, buffer size: {}", self.buffer.len());
                }
                DrawCommand::DrawRect(rect, paint) => {
                    use super::types::PaintFill;
                    let color = match &paint.fill {
                        PaintFill::Color(c) => *c,
                        _ => Color::rgb(0, 0, 0),
                    };
                    
                    trace!("DrawRect: ({}, {}) {}x{}, color=rgba({}, {}, {}, {})", 
                           rect.x, rect.y, rect.width, rect.height, color.r, color.g, color.b, color.a);
                    
                    match paint.style {
                        super::types::PaintStyle::Fill => {
                            self.draw_rect_filled(rect, color);
                        }
                        super::types::PaintStyle::Stroke => {
                            let stroke_width = paint.stroke_width as i32;
                            if stroke_width > 0 {
                                self.draw_rect_filled(
                                    Rect::new(rect.x, rect.y, rect.width, paint.stroke_width),
                                    color,
                                );
                                self.draw_rect_filled(
                                    Rect::new(rect.x, rect.y + rect.height - paint.stroke_width, rect.width, paint.stroke_width),
                                    color,
                                );
                                self.draw_rect_filled(
                                    Rect::new(rect.x, rect.y, paint.stroke_width, rect.height),
                                    color,
                                );
                                self.draw_rect_filled(
                                    Rect::new(rect.x + rect.width - paint.stroke_width, rect.y, paint.stroke_width, rect.height),
                                    color,
                                );
                            }
                        }
                    }
                }
                DrawCommand::DrawCircle(center, radius, paint) => {
                    let color = match &paint.fill {
                        PaintFill::Color(c) => *c,
                        _ => Color::rgb(0, 0, 0),
                    };
                    self.draw_circle_impl(center, radius, color, paint.style);
                }
                _ => {
                    // Other commands not yet implemented in software renderer
                }
            }
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
            let index = (y as u32 * self.width + x as u32) as usize;
            if index < self.buffer.len() {
                let rgba = ((color.a as u32) << 24)
                    | ((color.r as u32) << 16)
                    | ((color.g as u32) << 8)
                    | (color.b as u32);
                self.buffer[index] = rgba;
            }
        }
    }

    fn draw_rect_filled(&mut self, rect: Rect, color: Color) {
        let start_x = rect.x as i32;
        let start_y = rect.y as i32;
        let end_x = (rect.x + rect.width) as i32;
        let end_y = (rect.y + rect.height) as i32;

        for y in start_y.max(0)..end_y.min(self.height as i32) {
            for x in start_x.max(0)..end_x.min(self.width as i32) {
                self.set_pixel(x, y, color);
            }
        }
    }

    fn draw_circle_impl(&mut self, center: Point, radius: f32, color: Color, style: super::types::PaintStyle) {
        let r = radius as i32;
        let cx = center.x as i32;
        let cy = center.y as i32;

        match style {
            super::types::PaintStyle::Fill => {
                for y in (cy - r)..=(cy + r) {
                    for x in (cx - r)..=(cx + r) {
                        let dx = x - cx;
                        let dy = y - cy;
                        if dx * dx + dy * dy <= r * r {
                            self.set_pixel(x, y, color);
                        }
                    }
                }
            }
            super::types::PaintStyle::Stroke => {
                // Simple circle outline
                for angle in 0..360 {
                    let rad = (angle as f32).to_radians();
                    let x = cx + (radius * rad.cos()) as i32;
                    let y = cy + (radius * rad.sin()) as i32;
                    self.set_pixel(x, y, color);
                }
            }
        }
    }
}

impl Renderer for PlaceholderRenderer {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        info!("PlaceholderRenderer::init: {}x{}", info.width, info.height);
        self.width = info.width;
        self.height = info.height;
        let buffer_size = (self.width * self.height) as usize;
        info!("Allocating buffer: {} pixels ({} bytes)", buffer_size, buffer_size * 4);
        self.buffer = vec![0; buffer_size];
        self.initialized = true;
        info!("PlaceholderRenderer initialized");
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.width = width;
        self.height = height;
        self.buffer = vec![0; (self.width * self.height) as usize];
        Ok(())
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        debug!("PlaceholderRenderer::begin_frame");
        self.commands.clear();
        // Store a pointer to commands in thread-local storage
        COMMAND_BUFFER.with(|buf| {
            *buf.borrow_mut() = Some(&mut self.commands as *mut Vec<DrawCommand>);
        });
        debug!("Created drawing context, command buffer ready");
        Ok(Box::new(PlaceholderDrawingContext::new(
            self.width,
            self.height,
        )))
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        debug!("PlaceholderRenderer::end_frame");
        COMMAND_BUFFER.with(|buf| {
            *buf.borrow_mut() = None;
        });
        
        let command_count = self.commands.len();
        debug!("Applying {} drawing commands", command_count);
        self.apply_commands();
        
        // Store buffer pointer in thread-local for platform code to access
        // Always set it, even if empty, so platform code knows the state
        let (ptr, size) = self.get_buffer();
        let buffer_empty = self.buffer.is_empty();
        let ptr_null = ptr.is_null();
        
        debug!("Buffer state: empty={}, ptr_null={}, size={}, buffer_len={}", 
               buffer_empty, ptr_null, size, self.buffer.len());
        
        CURRENT_BUFFER.with(|buf| {
            if !self.buffer.is_empty() && !ptr.is_null() {
                info!("Storing buffer in thread-local: {}x{}, {} bytes", self.width, self.height, size);
                *buf.borrow_mut() = Some((ptr, size, self.width, self.height));
            } else {
                warn!("Not storing buffer - empty={}, ptr_null={}", buffer_empty, ptr_null);
                *buf.borrow_mut() = None;
            }
        });
        
        Ok(())
    }


    fn cleanup(&mut self) {
        self.initialized = false;
        self.buffer.clear();
    }
}

struct PlaceholderDrawingContext {
    transform_stack: Vec<Transform>,
    current_transform: Transform,
    alpha: f32,
    blend_mode: BlendMode,
    width: u32,
    height: u32,
    commands: Vec<DrawCommand>,
}

impl PlaceholderDrawingContext {
    fn new(width: u32, height: u32) -> Self {
        Self {
            transform_stack: Vec::new(),
            current_transform: Transform::identity(),
            alpha: 1.0,
            blend_mode: BlendMode::Normal,
            width,
            height,
            commands: Vec::new(),
        }
    }
    
}

impl DrawingContext for PlaceholderDrawingContext {
    fn clear(&mut self, color: Color) -> AureaResult<()> {
        COMMAND_BUFFER.with(|buf| {
            if let Some(ptr) = *buf.borrow() {
                unsafe {
                    (*ptr).push(DrawCommand::Clear(color));
                }
            }
        });
        Ok(())
    }

    fn draw_rect(&mut self, rect: Rect, paint: &Paint) -> AureaResult<()> {
        COMMAND_BUFFER.with(|buf| {
            if let Some(ptr) = *buf.borrow() {
                unsafe {
                    (*ptr).push(DrawCommand::DrawRect(rect, paint.clone()));
                }
            }
        });
        Ok(())
    }

    fn draw_circle(&mut self, center: Point, radius: f32, paint: &Paint) -> AureaResult<()> {
        COMMAND_BUFFER.with(|buf| {
            if let Some(ptr) = *buf.borrow() {
                unsafe {
                    (*ptr).push(DrawCommand::DrawCircle(center, radius, paint.clone()));
                }
            }
        });
        Ok(())
    }

    fn draw_text(&mut self, text: &str, position: Point, paint: &Paint) -> AureaResult<()> {
        self.commands.push(DrawCommand::DrawText(text.to_string(), position, paint.clone()));
        Ok(())
    }

    fn draw_path(&mut self, path: &Path, paint: &Paint) -> AureaResult<()> {
        self.commands.push(DrawCommand::DrawPath(path.clone(), paint.clone()));
        Ok(())
    }

    fn draw_image(&mut self, _image: &Image, _position: Point) -> AureaResult<()> {
        Ok(())
    }

    fn draw_image_rect(&mut self, _image: &Image, _dest: Rect) -> AureaResult<()> {
        Ok(())
    }

    fn draw_image_region(&mut self, _image: &Image, _src: Rect, _dest: Rect) -> AureaResult<()> {
        Ok(())
    }

    fn draw_text_with_font(&mut self, text: &str, position: Point, font: &Font, paint: &Paint) -> AureaResult<()> {
        self.commands.push(DrawCommand::DrawTextWithFont(text.to_string(), position, font.clone(), paint.clone()));
        Ok(())
    }

    fn measure_text(&mut self, _text: &str, _font: &Font) -> AureaResult<TextMetrics> {
        Ok(TextMetrics {
            width: 0.0,
            height: 0.0,
            ascent: 0.0,
            descent: 0.0,
            advance: 0.0,
        })
    }

    fn save(&mut self) -> AureaResult<()> {
        self.transform_stack.push(self.current_transform);
        Ok(())
    }

    fn restore(&mut self) -> AureaResult<()> {
        if let Some(transform) = self.transform_stack.pop() {
            self.current_transform = transform;
        }
        Ok(())
    }

    fn transform(&mut self, transform: Transform) -> AureaResult<()> {
        self.current_transform = self.current_transform.multiply(transform);
        Ok(())
    }

    fn clip_rect(&mut self, _rect: Rect) -> AureaResult<()> {
        Ok(())
    }

    fn clip_path(&mut self, _path: &Path) -> AureaResult<()> {
        Ok(())
    }

    fn set_alpha(&mut self, alpha: f32) -> AureaResult<()> {
        self.alpha = alpha.clamp(0.0, 1.0);
        Ok(())
    }

    fn set_blend_mode(&mut self, mode: BlendMode) -> AureaResult<()> {
        self.blend_mode = mode;
        Ok(())
    }

    fn fill_linear_gradient(&mut self, _gradient: &LinearGradient, _rect: Rect) -> AureaResult<()> {
        Ok(())
    }

    fn fill_radial_gradient(&mut self, _gradient: &RadialGradient, _rect: Rect) -> AureaResult<()> {
        Ok(())
    }

    fn hit_test_path(&mut self, _path: &Path, _point: Point) -> AureaResult<bool> {
        Ok(false)
    }
}


