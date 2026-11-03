//! A drawing kept as pixels: drawn once, then one image draw every frame after, on any canvas.

use crate::cpu::CpuRasterizer;
use crate::drawing::DrawingContext;
use crate::types::{Image, Point, Rect};
use aurea_foundation::{AureaError, AureaResult};

/// What `draw` drew, at one scale. Good for art that is costly and rarely changes, such as a
/// colour ring of dozens of paths. Record it again when its content or the scale changes.
///
/// ```no_run
/// # use aurea_render::{Picture, DrawingContext, Point};
/// # fn f(ctx: &mut dyn DrawingContext) -> aurea_foundation::AureaResult<()> {
/// let ring = Picture::record(200, 200, 2.0, |ctx| ctx.clear(aurea_render::Color::rgb(9, 9, 9)))?;
/// ring.draw(ctx, Point::new(10.0, 10.0))?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct Picture {
    image: Image,
    width: u32,
    height: u32,
    scale: f32,
}

impl Picture {
    /// Runs `draw` on a clear canvas of `width` by `height` logical pixels at `scale`.
    pub fn record(
        width: u32,
        height: u32,
        scale: f32,
        draw: impl FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    ) -> AureaResult<Self> {
        if width == 0 || height == 0 {
            return Err(AureaError::RenderingFailed);
        }
        let pixels = CpuRasterizer::draw_offscreen(width, height, scale, draw)?;
        Ok(Self {
            image: Image::new(pixels.width, pixels.height, pixels.rgba),
            width,
            height,
            scale,
        })
    }

    /// The scale it was recorded at. Sharp only when drawn at that scale, at a whole device pixel.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Its size in logical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Draws it with its top left at `at`.
    pub fn draw(&self, ctx: &mut dyn DrawingContext, at: Point) -> AureaResult<()> {
        let rect = Rect::new(at.x, at.y, self.width as f32, self.height as f32);
        ctx.draw_image_rect(&self.image, rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Color, Paint};

    #[test]
    fn a_picture_draws_what_was_recorded_where_it_is_asked() {
        let scene = |ctx: &mut dyn DrawingContext, x: f32| {
            let paint = Paint::new().color(Color::rgba(200, 80, 40, 180));
            ctx.draw_circle(Point::new(x + 20.0, 20.0), 14.0, &paint)
        };
        let picture = Picture::record(40, 40, 2.0, |ctx| scene(ctx, 0.0)).expect("record");
        assert_eq!((picture.size(), picture.scale()), ((40, 40), 2.0));
        let baked = CpuRasterizer::draw_offscreen(100, 40, 2.0, |ctx| {
            picture.draw(ctx, Point::new(30.0, 0.0))
        })
        .expect("draw");
        let direct =
            CpuRasterizer::draw_offscreen(100, 40, 2.0, |ctx| scene(ctx, 30.0)).expect("draw");
        assert_eq!((baked.width, baked.height), (direct.width, direct.height));
        let worst = baked
            .rgba
            .iter()
            .zip(&direct.rgba)
            .map(|(a, b)| a.abs_diff(*b))
            .max();
        assert!(worst <= Some(2), "{worst:?}");
    }

    #[test]
    fn an_empty_picture_is_refused() {
        assert!(Picture::record(0, 10, 1.0, |_| Ok(())).is_err());
    }
}
