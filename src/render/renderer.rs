use super::surface::{Surface, SurfaceInfo};
use super::types::{
    BlendMode, Color, Font, Image, LinearGradient, Paint, Path, Point, RadialGradient, Rect,
    TextMetrics, Transform,
};
use crate::AureaResult;
use std::cell::RefCell;

thread_local! {
    static COMMAND_BUFFER: RefCell<Option<*mut Vec<DrawCommand>>> = const { RefCell::new(None) };
    pub(crate) static CURRENT_BUFFER: RefCell<Option<(*const u8, usize, u32, u32)>> = const { RefCell::new(None) };
}

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
    fn draw_text_with_font(
        &mut self,
        text: &str,
        position: Point,
        font: &Font,
        paint: &Paint,
    ) -> AureaResult<()>;

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

pub trait Renderer: Send + Sync {
    fn init(&mut self, surface: Surface, info: SurfaceInfo) -> AureaResult<()>;
    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()>;
    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>>;
    fn end_frame(&mut self) -> AureaResult<()>;
    fn cleanup(&mut self);

    /// Set damage region for partial redraw (optional, defaults to full canvas)
    /// This is called before end_frame() to specify which region needs redrawing
    fn set_damage(&mut self, _damage: Option<Rect>) {
        // Default implementation does nothing (full redraw)
    }
}

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Clear(Color),
    DrawRect(Rect, Paint),
    DrawCircle(Point, f32, Paint),
    #[allow(dead_code)]
    DrawPath(Path, Paint),
    #[allow(dead_code)]
    DrawText(String, Point, Paint),
    #[allow(dead_code)]
    DrawTextWithFont(String, Point, Font, Paint),
    // Stack operations for compositing
    PushClip(Path),
    PopClip,
    PushTransform(Transform),
    PopTransform,
    PushOpacity(f32),
    PopOpacity,
}

#[derive(Default)]
pub struct PlaceholderRenderer {
    initialized: bool,
    width: u32,
    height: u32,
    buffer: Vec<u32>,
    commands: Vec<DrawCommand>,
}

impl PlaceholderRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_buffer(&self) -> (*const u8, usize) {
        if self.buffer.is_empty() {
            return (std::ptr::null(), 0);
        }
        // Convert u32 buffer to u8 pointer (same memory, just different type)
        (
            self.buffer.as_ptr() as *const u8,
            self.buffer.len() * std::mem::size_of::<u32>(),
        )
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn apply_commands(&mut self) {
        let commands = std::mem::take(&mut self.commands);
        for cmd in commands.into_iter() {
            match cmd {
                DrawCommand::Clear(color) => {
                    let rgba = ((color.a as u32) << 24)
                        | ((color.r as u32) << 16)
                        | ((color.g as u32) << 8)
                        | (color.b as u32);
                    self.buffer.fill(rgba);
                }
                DrawCommand::DrawRect(rect, paint) => {
                    let color = paint.color;

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
                                    Rect::new(
                                        rect.x,
                                        rect.y + rect.height - paint.stroke_width,
                                        rect.width,
                                        paint.stroke_width,
                                    ),
                                    color,
                                );
                                self.draw_rect_filled(
                                    Rect::new(rect.x, rect.y, paint.stroke_width, rect.height),
                                    color,
                                );
                                self.draw_rect_filled(
                                    Rect::new(
                                        rect.x + rect.width - paint.stroke_width,
                                        rect.y,
                                        paint.stroke_width,
                                        rect.height,
                                    ),
                                    color,
                                );
                            }
                        }
                    }
                }
                DrawCommand::DrawCircle(center, radius, paint) => {
                    let color = paint.color;
                    self.draw_circle_impl(center, radius, color, paint.style);
                }
                DrawCommand::DrawText(..) => {}
                DrawCommand::DrawPath(..) => {}
                DrawCommand::DrawTextWithFont(..) => {}
                DrawCommand::PushClip(..) => {}
                DrawCommand::PopClip => {}
                DrawCommand::PushTransform(..) => {}
                DrawCommand::PopTransform => {}
                DrawCommand::PushOpacity(..) => {}
                DrawCommand::PopOpacity => {}
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

    fn draw_circle_impl(
        &mut self,
        center: Point,
        radius: f32,
        color: Color,
        style: super::types::PaintStyle,
    ) {
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
        self.width = info.width;
        self.height = info.height;
        self.buffer = vec![0; (self.width * self.height) as usize];
        self.initialized = true;
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.width = width;
        self.height = height;
        self.buffer = vec![0; (self.width * self.height) as usize];
        Ok(())
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        self.commands.clear();
        COMMAND_BUFFER.with(|buf| {
            *buf.borrow_mut() = Some(&mut self.commands as *mut Vec<DrawCommand>);
        });
        Ok(Box::new(PlaceholderDrawingContext::new(
            self.width,
            self.height,
        )))
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        COMMAND_BUFFER.with(|buf| {
            *buf.borrow_mut() = None;
        });

        self.apply_commands();

        let (ptr, size) = self.get_buffer();
        CURRENT_BUFFER.with(|buf| {
            if !self.buffer.is_empty() && !ptr.is_null() {
                *buf.borrow_mut() = Some((ptr, size, self.width, self.height));
            } else {
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
    _width: u32,
    _height: u32,
    commands: Vec<DrawCommand>,
}

impl PlaceholderDrawingContext {
    fn new(width: u32, height: u32) -> Self {
        Self {
            transform_stack: Vec::new(),
            current_transform: Transform::identity(),
            alpha: 1.0,
            blend_mode: BlendMode::Normal,
            _width: width,
            _height: height,
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
        self.commands.push(DrawCommand::DrawText(
            text.to_string(),
            position,
            paint.clone(),
        ));
        Ok(())
    }

    fn draw_path(&mut self, path: &Path, paint: &Paint) -> AureaResult<()> {
        self.commands
            .push(DrawCommand::DrawPath(path.clone(), paint.clone()));
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

    fn draw_text_with_font(
        &mut self,
        text: &str,
        position: Point,
        font: &Font,
        paint: &Paint,
    ) -> AureaResult<()> {
        self.commands.push(DrawCommand::DrawTextWithFont(
            text.to_string(),
            position,
            font.clone(),
            paint.clone(),
        ));
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
