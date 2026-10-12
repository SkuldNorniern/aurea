use crate::display_list::DisplayList;
use crate::surface::{Surface, SurfaceInfo};
use crate::types::{
    BlendMode, Color, Font, Image, LinearGradient, Paint, Path, PathCommand, Point, RadialGradient,
    Rect, TextMetrics, Transform,
};
use aurea_foundation::AureaResult;
use std::marker::PhantomData;
use std::slice;

/// A renderer's finished pixels, borrowed for as long as the renderer is.
///
/// The pointer is the renderer's own buffer. It stays valid until the
/// renderer next draws or is resized, which is why this borrows it.
#[derive(Debug, Clone, Copy)]
pub struct FrameOutput<'a> {
    /// Start of the pixel data.
    pub pixels: *const u8,
    /// Length in bytes.
    pub size: usize,
    pub width: u32,
    pub height: u32,
    _borrow: PhantomData<&'a [u8]>,
}

impl<'a> FrameOutput<'a> {
    /// Describes `pixels`, which must stay valid as long as `'a`.
    pub fn new(pixels: *const u8, size: usize, width: u32, height: u32) -> Option<Self> {
        if pixels.is_null() || size == 0 {
            return None;
        }
        Some(Self {
            pixels,
            size,
            width,
            height,
            _borrow: PhantomData,
        })
    }
    /// The pixel bytes.
    pub fn bytes(&self) -> &'a [u8] {
        // SAFETY: `new` takes pixels that stay valid, `size` bytes long, for
        // as long as `'a`, which is what this borrows.
        unsafe { slice::from_raw_parts(self.pixels, self.size) }
    }
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

    /// Draw a rectangle with rounded corners, `radius` kept to half the
    /// shorter side. A stroke is centred on the outline.
    ///
    /// Contexts that can draw one directly do; this default draws the
    /// outline as a path.
    fn draw_round_rect(&mut self, rect: Rect, radius: f32, paint: &Paint) -> AureaResult<()> {
        self.draw_path(&Path::rounded_rect(rect, radius), paint)
    }

    /// Draw a circle
    fn draw_circle(&mut self, center: Point, radius: f32, paint: &Paint) -> AureaResult<()>;

    /// Draw text at a position
    fn draw_text(&mut self, text: &str, position: Point, paint: &Paint) -> AureaResult<()>;

    /// Draw a line between two points
    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, paint: &Paint) -> AureaResult<()> {
        let mut path = Path::new();
        path.commands.push(PathCommand::MoveTo(Point::new(x1, y1)));
        path.commands.push(PathCommand::LineTo(Point::new(x2, y2)));
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

/// A renderer belongs to the thread that draws with it.
///
/// Not `Send` or `Sync`: it holds a native surface, and the toolkits behind
/// those are thread-affine. It used to require both, which forced everything
/// reachable from a draw callback to be `Send + Sync` as well — including the
/// canvas, which then had to be shared through a mutex that bought nothing.
/// What [`Renderer::end_frame`] did with the frame, so whoever drives the
/// renderer knows whether to try again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameOutcome {
    /// New pixels are on their way to the target.
    Presented,
    /// Nothing changed since the last frame; what the target shows is
    /// still right.
    Unchanged,
    /// The target could not take a frame now (out of date, timed out,
    /// hidden). Draw again on the next frame.
    Retry,
    /// The target is gone and the renderer must be made again before it
    /// can present.
    Recreate,
    /// The target has no size. Nothing is drawn until it is configured with
    /// one again.
    Suspended,
}

impl FrameOutcome {
    /// Whether the frame still has to be drawn later.
    pub fn needs_redraw(self) -> bool {
        matches!(self, Self::Retry | Self::Recreate | Self::Suspended)
    }
}

pub trait Renderer {
    fn init(&mut self, surface: Surface, info: SurfaceInfo) -> AureaResult<()>;
    /// A new logical size at the current scale factor.
    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()>;
    /// A new logical size and scale factor together: the one call to make
    /// whenever either changes. A scale change alone resizes the pixel
    /// target, which a separate `init` and `resize` left to chance. A zero
    /// size suspends the renderer until it gets a real one.
    fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()>;
    /// Starts a frame and hands back a context to record into.
    ///
    /// The context borrows the renderer, so it cannot outlive it and the
    /// renderer cannot be touched again until the context is dropped.
    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext + '_>>;
    fn end_frame(&mut self) -> AureaResult<FrameOutcome>;
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

    /// Physical-pixel rect that was repainted by the last `end_frame` call.
    /// `None` means the whole frame was repainted (or unknown). Used by the
    /// platform layer to compute how much of the IOSurface double-buffer needs
    /// refreshing; defaults to `None` for renderers that don't track tile damage.
    /// The pixels this renderer produced, if it keeps any.
    ///
    /// Asked of a renderer rather than read from an ambient slot, so what
    /// comes back belongs to this renderer. The slot was per thread, and with
    /// two canvases on the UI thread the one that rendered last was what the
    /// other published: a canvas with no draw callback would put its
    /// neighbour's pixels on screen.
    ///
    /// `None` for a renderer that presents its own frames, such as a GPU
    /// backend, and for one whose buffer is gone after a resize.
    fn frame_output(&self) -> Option<FrameOutput<'_>> {
        None
    }

    fn last_frame_damage(&self) -> Option<Rect> {
        None
    }
}
