use std::os::raw::c_void;
use crate::elements::Element;
use crate::{AureaError, AureaResult};
use crate::ffi::*;
use crate::platform::Platform;
use crate::capability::CapabilityChecker;
use super::renderer::{Renderer, DrawingContext, PlaceholderRenderer};
use super::types::RendererBackend;
use super::surface::{Surface, SurfaceInfo};

pub struct Canvas {
    handle: *mut c_void,
    renderer: Option<Box<dyn Renderer>>,
    _backend: RendererBackend,
    width: u32,
    height: u32,
    #[allow(dead_code)]
    platform: Platform,
    #[allow(dead_code)]
    capabilities: CapabilityChecker,
}

impl Canvas {
    pub fn new(width: u32, height: u32, backend: RendererBackend) -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_canvas(width as i32, height as i32) };
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        let mut renderer: Box<dyn Renderer> = Box::new(PlaceholderRenderer::new());
        let surface = Surface::OpenGL { context: std::ptr::null_mut() };
        let surface_info = SurfaceInfo {
            width,
            height,
            scale_factor: 1.0,
        };
        
        renderer.init(surface, surface_info)?;

        let platform = Platform::current();
        let capabilities = CapabilityChecker::new();
        Ok(Self {
            handle,
            renderer: Some(renderer),
            _backend: backend,
            width,
            height,
            platform,
            capabilities,
        })
    }

    pub fn draw<F>(&mut self, draw_fn: F) -> AureaResult<()>
    where
        F: FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    {
        if let Some(ref mut renderer) = self.renderer {
            let mut ctx = renderer.begin_frame()?;
            draw_fn(ctx.as_mut())?;
            renderer.end_frame()?;
            self.update_platform_view();
        }
        Ok(())
    }

    fn update_platform_view(&self) {
        if let Some((ptr, size, width, height)) = self.get_render_buffer()
            && !ptr.is_null() && size > 0
        {
            unsafe {
                ng_platform_canvas_update_buffer(self.handle, ptr, size as u32, width, height);
            }
        }
    }

    pub fn invalidate(&self) {
        unsafe {
            ng_platform_canvas_invalidate(self.handle);
        }
    }

    fn get_render_buffer(&self) -> Option<(*const u8, usize, u32, u32)> {
        use crate::render::renderer::CURRENT_BUFFER;
        CURRENT_BUFFER.with(|buf| *buf.borrow())
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

impl Element for Canvas {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        if let Some(ref mut renderer) = self.renderer {
            renderer.cleanup();
        }
    }
}

