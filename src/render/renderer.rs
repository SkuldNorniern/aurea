use super::display_list::DisplayList;
use super::surface::{Surface, SurfaceInfo};
use super::types::{
    BlendMode, Color, Font, Image, LinearGradient, Paint, Path, Point, RadialGradient, Rect,
    TextMetrics, Transform,
};
use crate::AureaResult;
use crate::render::text::TextRenderer;
use std::cell::RefCell;
use std::sync::LazyLock;

thread_local! {
    static COMMAND_BUFFER: RefCell<Option<*mut Vec<DrawCommand>>> = const { RefCell::new(None) };
    pub static CURRENT_BUFFER: RefCell<Option<(*const u8, usize, u32, u32)>> = const { RefCell::new(None) };
}

pub trait DrawingContext {
    /// Get the width of the drawing area
    fn width(&self) -> u32;

    /// Get the height of the drawing area
    fn height(&self) -> u32;

    /// Clear the canvas with a color
    fn clear(&mut self, color: Color) -> AureaResult<()>;

    /// Draw a rectangle
    fn draw_rect(&mut self, rect: Rect, paint: &Paint) -> AureaResult<()>;

    /// Draw a circle
    fn draw_circle(&mut self, center: Point, radius: f32, paint: &Paint) -> AureaResult<()>;

    /// Draw text at a position
    fn draw_text(&mut self, text: &str, position: Point, paint: &Paint) -> AureaResult<()>;

    /// Draw a line between two points
    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, paint: &Paint) -> AureaResult<()> {
        let mut path = Path::new();
        path.commands
            .push(super::types::PathCommand::MoveTo(Point::new(x1, y1)));
        path.commands
            .push(super::types::PathCommand::LineTo(Point::new(x2, y2)));
        self.draw_path(&path, paint)
    }

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

    /// Optional: access the display list for hit testing (CPU renderer only).
    fn display_list(&self) -> Option<&DisplayList> {
        None
    }
}

static PLACEHOLDER_TEXT_RENDERER: LazyLock<TextRenderer> = LazyLock::new(TextRenderer::new);
const DEFAULT_FONT_FAMILY: &str = "Sans";
const DEFAULT_FONT_SIZE: f32 = 16.0;

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
    DrawImageRect(Image, Rect),
    DrawImageRegion(Image, Rect, Rect),
    FillLinearGradient(LinearGradient, Rect),
    FillRadialGradient(RadialGradient, Rect),
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
                DrawCommand::DrawImageRect(image, dest) => {
                    Self::blit_image_to_buffer(
                        &image.data,
                        image.width,
                        image.height,
                        Rect::new(0.0, 0.0, image.width as f32, image.height as f32),
                        dest,
                        &mut self.buffer,
                        self.width,
                        self.height,
                    );
                }
                DrawCommand::DrawImageRegion(image, src, dest) => {
                    Self::blit_image_to_buffer(
                        &image.data,
                        image.width,
                        image.height,
                        src,
                        dest,
                        &mut self.buffer,
                        self.width,
                        self.height,
                    );
                }
                DrawCommand::FillLinearGradient(gradient, rect) => {
                    Self::fill_linear_gradient_buffer(
                        &gradient,
                        rect,
                        &mut self.buffer,
                        self.width,
                        self.height,
                    );
                }
                DrawCommand::FillRadialGradient(gradient, rect) => {
                    Self::fill_radial_gradient_buffer(
                        &gradient,
                        rect,
                        &mut self.buffer,
                        self.width,
                        self.height,
                    );
                }
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

    fn blit_image_to_buffer(
        image_data: &[u8],
        image_width: u32,
        image_height: u32,
        src: Rect,
        dest: Rect,
        buffer: &mut [u32],
        buffer_width: u32,
        buffer_height: u32,
    ) {
        if image_data.is_empty()
            || image_width == 0
            || image_height == 0
            || dest.width <= 0.0
            || dest.height <= 0.0
        {
            return;
        }
        let _stride = (image_width * 4) as usize;
        let start_x = dest.x as i32;
        let start_y = dest.y as i32;
        let end_x = (dest.x + dest.width) as i32;
        let end_y = (dest.y + dest.height) as i32;
        for dy in start_y..end_y {
            for dx in start_x..end_x {
                if dx < 0 || dy < 0 || dx >= buffer_width as i32 || dy >= buffer_height as i32 {
                    continue;
                }
                let u = (dx - start_x) as f32 / dest.width * src.width + src.x;
                let v = (dy - start_y) as f32 / dest.height * src.height + src.y;
                let sx = u.clamp(0.0, image_width as f32 - 0.001) as u32;
                let sy = v.clamp(0.0, image_height as f32 - 0.001) as u32;
                let idx = (sy as usize * image_width as usize + sx as usize) * 4;
                if idx + 3 >= image_data.len() {
                    continue;
                }
                let r = image_data[idx];
                let g = image_data[idx + 1];
                let b = image_data[idx + 2];
                let a = image_data[idx + 3];
                let src_rgba =
                    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                let buf_idx = (dy as u32 * buffer_width + dx as u32) as usize;
                if buf_idx >= buffer.len() {
                    continue;
                }
                if a >= 255 {
                    buffer[buf_idx] = src_rgba;
                } else {
                    let dst = buffer[buf_idx];
                    let da = (dst >> 24) & 0xff;
                    let dr = (dst >> 16) & 0xff;
                    let dg = (dst >> 8) & 0xff;
                    let db = dst & 0xff;
                    let sa = a as u32;
                    let inv_sa = (255 - sa) as u32;
                    let out_a = sa + (inv_sa * da) / 255;
                    if out_a == 0 {
                        buffer[buf_idx] = 0;
                    } else {
                        let out_r = (sa * (r as u32) + inv_sa * dr) / 255;
                        let out_g = (sa * (g as u32) + inv_sa * dg) / 255;
                        let out_b = (sa * (b as u32) + inv_sa * db) / 255;
                        buffer[buf_idx] = (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b;
                    }
                }
            }
        }
    }

    fn gradient_color_at(stops: &[super::types::GradientStop], t: f32) -> super::types::Color {
        let t = t.clamp(0.0, 1.0);
        if stops.is_empty() {
            return super::types::Color::rgb(0, 0, 0);
        }
        if stops.len() == 1 {
            return stops[0].color;
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
                return super::types::Color::rgba(
                    (c0.r as f32 + (c1.r as f32 - c0.r as f32) * s).round() as u8,
                    (c0.g as f32 + (c1.g as f32 - c0.g as f32) * s).round() as u8,
                    (c0.b as f32 + (c1.b as f32 - c0.b as f32) * s).round() as u8,
                    (c0.a as f32 + (c1.a as f32 - c0.a as f32) * s).round() as u8,
                );
            }
        }
        if t <= stops[0].offset {
            stops[0].color
        } else {
            *stops.last().map(|s| &s.color).unwrap_or(&stops[0].color)
        }
    }

    fn fill_linear_gradient_buffer(
        gradient: &LinearGradient,
        rect: Rect,
        buffer: &mut [u32],
        buffer_width: u32,
        buffer_height: u32,
    ) {
        let dx = gradient.end.x - gradient.start.x;
        let dy = gradient.end.y - gradient.start.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq < 1e-10 {
            return;
        }
        let start_x = rect.x.max(0.0).min(buffer_width as f32) as i32;
        let end_x = (rect.x + rect.width).max(0.0).min(buffer_width as f32) as i32;
        let start_y = rect.y.max(0.0).min(buffer_height as f32) as i32;
        let end_y = (rect.y + rect.height).max(0.0).min(buffer_height as f32) as i32;
        for py in start_y..end_y {
            for px in start_x..end_x {
                let px_f = px as f32 + 0.5;
                let py_f = py as f32 + 0.5;
                let t = ((px_f - gradient.start.x) * dx + (py_f - gradient.start.y) * dy) / len_sq;
                let t = t.clamp(0.0, 1.0);
                let color = Self::gradient_color_at(&gradient.stops, t);
                let idx = (py as u32 * buffer_width + px as u32) as usize;
                if idx < buffer.len() {
                    let rgba = ((color.a as u32) << 24)
                        | ((color.r as u32) << 16)
                        | ((color.g as u32) << 8)
                        | (color.b as u32);
                    buffer[idx] = rgba;
                }
            }
        }
    }

    fn fill_radial_gradient_buffer(
        gradient: &RadialGradient,
        rect: Rect,
        buffer: &mut [u32],
        buffer_width: u32,
        buffer_height: u32,
    ) {
        if gradient.radius <= 0.0 {
            return;
        }
        let start_x = rect.x.max(0.0).min(buffer_width as f32) as i32;
        let end_x = (rect.x + rect.width).max(0.0).min(buffer_width as f32) as i32;
        let start_y = rect.y.max(0.0).min(buffer_height as f32) as i32;
        let end_y = (rect.y + rect.height).max(0.0).min(buffer_height as f32) as i32;
        for py in start_y..end_y {
            for px in start_x..end_x {
                let px_f = px as f32 + 0.5;
                let py_f = py as f32 + 0.5;
                let dist = ((px_f - gradient.center.x).powi(2)
                    + (py_f - gradient.center.y).powi(2))
                .sqrt();
                let t = (dist / gradient.radius).min(1.0);
                let color = Self::gradient_color_at(&gradient.stops, t);
                let idx = (py as u32 * buffer_width + px as u32) as usize;
                if idx < buffer.len() {
                    let rgba = ((color.a as u32) << 24)
                        | ((color.r as u32) << 16)
                        | ((color.g as u32) << 8)
                        | (color.b as u32);
                    buffer[idx] = rgba;
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
    fn width(&self) -> u32 {
        self._width
    }

    fn height(&self) -> u32 {
        self._height
    }

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
        let font = Font::new(DEFAULT_FONT_FAMILY, DEFAULT_FONT_SIZE);
        self.draw_text_with_font(text, position, &font, paint)
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
        if text.is_empty() {
            return Ok(());
        }

        let metrics = PLACEHOLDER_TEXT_RENDERER.measure_text(text, font)?;
        if metrics.width <= 0.0 || metrics.height <= 0.0 {
            return Ok(());
        }

        const TEXT_PADDING: f32 = 8.0;
        let width = (metrics.width.ceil().max(1.0) + TEXT_PADDING * 2.0) as u32;
        let height = (metrics.height.ceil().max(1.0) + TEXT_PADDING * 2.0) as u32;
        let mut buffer = vec![0u32; (width * height) as usize];
        let origin = Point::new(TEXT_PADDING, TEXT_PADDING + metrics.ascent.max(0.0));

        PLACEHOLDER_TEXT_RENDERER.render_text(
            text,
            origin,
            font,
            paint.color,
            &mut buffer,
            width,
            height,
        )?;

        let mut data = Vec::with_capacity(buffer.len() * 4);
        for pixel in buffer {
            let a = ((pixel >> 24) & 0xFF) as u8;
            let r = ((pixel >> 16) & 0xFF) as u8;
            let g = ((pixel >> 8) & 0xFF) as u8;
            let b = (pixel & 0xFF) as u8;
            data.push(r);
            data.push(g);
            data.push(b);
            data.push(a);
        }

        let image = Image::new(width, height, data);
        let dest = Rect::new(
            position.x - TEXT_PADDING,
            position.y - (TEXT_PADDING + metrics.ascent),
            width as f32,
            height as f32,
        );
        self.commands.push(DrawCommand::DrawImageRect(image, dest));
        Ok(())
    }

    fn measure_text(&mut self, text: &str, font: &Font) -> AureaResult<TextMetrics> {
        if text.is_empty() {
            return Ok(TextMetrics {
                width: 0.0,
                height: 0.0,
                ascent: 0.0,
                descent: 0.0,
                advance: 0.0,
            });
        }
        PLACEHOLDER_TEXT_RENDERER.measure_text(text, font)
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

    fn fill_linear_gradient(&mut self, gradient: &LinearGradient, rect: Rect) -> AureaResult<()> {
        COMMAND_BUFFER.with(|buf| {
            if let Some(ptr) = *buf.borrow() {
                unsafe {
                    (*ptr).push(DrawCommand::FillLinearGradient(gradient.clone(), rect));
                }
            }
        });
        Ok(())
    }

    fn fill_radial_gradient(&mut self, gradient: &RadialGradient, rect: Rect) -> AureaResult<()> {
        COMMAND_BUFFER.with(|buf| {
            if let Some(ptr) = *buf.borrow() {
                unsafe {
                    (*ptr).push(DrawCommand::FillRadialGradient(gradient.clone(), rect));
                }
            }
        });
        Ok(())
    }

    fn hit_test_path(&mut self, _path: &Path, _point: Point) -> AureaResult<bool> {
        Ok(false)
    }
}
