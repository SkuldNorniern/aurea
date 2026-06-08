//! GPU backend for the renderer.
//!
//! Provides a Renderer implementation behind RendererBackend::Gpu.
//! Currently delegates to the CPU rasterizer; a wgpu-based pipeline can be
//! implemented later for true GPU acceleration.

use super::cpu::CpuRasterizer;
use super::display_list::DisplayList;
use super::renderer::{DrawingContext, Renderer};
use super::surface::{Surface, SurfaceInfo};
use super::types::Rect;
use aurea_core::AureaResult;

/// GPU rasterizer. Implements the Renderer trait for RendererBackend::Gpu.
/// Uses CPU rasterization until a wgpu pipeline is implemented.
pub struct GpuRasterizer {
    inner: CpuRasterizer,
}

impl GpuRasterizer {
    /// Creates a GPU rasterizer for the given canvas size.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            inner: CpuRasterizer::new(width, height),
        }
    }
}

impl Renderer for GpuRasterizer {
    fn init(&mut self, surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.inner.init(surface, info)
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.inner.resize(width, height);
        Ok(())
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        self.inner.begin_frame()
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        self.inner.end_frame()
    }

    fn cleanup(&mut self) {
        self.inner.cleanup()
    }

    fn set_damage(&mut self, damage: Option<Rect>) {
        self.inner.set_damage(damage)
    }

    fn display_list(&self) -> Option<&DisplayList> {
        Some(self.inner.display_list())
    }
}

#[cfg(test)]
impl GpuRasterizer {
    /// Read back the frame buffer for test assertions.
    fn read_buffer(&self) -> Vec<u32> {
        let (ptr, size, w, h) = self.inner.get_buffer();
        let n = (w * h) as usize;
        assert_eq!(size, n * 4);
        unsafe { std::slice::from_raw_parts(ptr as *const u32, n).to_vec() }
    }
}

#[cfg(test)]
mod tests {
    use super::super::surface::Surface;
    use super::super::types::{Color, Paint, Rect};
    use super::*;

    #[test]
    fn gpu_rasterizer_init_and_frame() {
        let mut r = GpuRasterizer::new(32, 32);
        r.init(Surface::OpenGL { context: std::ptr::null_mut() },
               SurfaceInfo { width: 32, height: 32, scale_factor: 1.0 })
         .expect("init");
        let mut ctx = r.begin_frame().expect("begin_frame");
        ctx.clear(Color::rgb(255, 0, 0)).expect("clear");
        drop(ctx);
        r.end_frame().expect("end_frame");
        r.cleanup();
    }

    #[test]
    fn gpu_rasterizer_clear_produces_uniform_color() {
        let mut r = GpuRasterizer::new(64, 64);
        r.init(Surface::OpenGL { context: std::ptr::null_mut() },
               SurfaceInfo { width: 64, height: 64, scale_factor: 1.0 })
         .expect("init");
        let mut ctx = r.begin_frame().expect("begin_frame");
        ctx.clear(Color::rgb(0xFF, 0x00, 0x00)).expect("clear");
        drop(ctx);
        r.end_frame().expect("end_frame");

        let buf = r.read_buffer();
        let center = buf[32 * 64 + 32];
        let expected = (255u32 << 24) | (255 << 16);
        assert_eq!(center, expected, "center pixel should be red");
    }

    #[test]
    fn gpu_rasterizer_same_scene_deterministic_output() {
        let mut r = GpuRasterizer::new(32, 32);
        r.init(Surface::OpenGL { context: std::ptr::null_mut() },
               SurfaceInfo { width: 32, height: 32, scale_factor: 1.0 })
         .expect("init");

        for _ in 0..2 {
            let mut ctx = r.begin_frame().expect("begin_frame");
            ctx.clear(Color::rgb(0, 128, 255)).expect("clear");
            ctx.draw_rect(Rect::new(8.0, 8.0, 16.0, 16.0),
                          &Paint::new().color(Color::rgb(255, 255, 0)))
               .expect("draw_rect");
            drop(ctx);
            r.end_frame().expect("end_frame");
        }
        let buf1 = r.read_buffer();

        let mut ctx = r.begin_frame().expect("begin_frame");
        ctx.clear(Color::rgb(0, 128, 255)).expect("clear");
        ctx.draw_rect(Rect::new(8.0, 8.0, 16.0, 16.0),
                      &Paint::new().color(Color::rgb(255, 255, 0)))
           .expect("draw_rect");
        drop(ctx);
        r.end_frame().expect("end_frame");
        let buf2 = r.read_buffer();

        assert_eq!(buf1, buf2, "same scene must produce identical output");
    }
}
