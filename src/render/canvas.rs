use std::os::raw::c_void;
use std::sync::Mutex;
use crate::elements::Element;
use crate::{AureaError, AureaResult};
use crate::ffi::*;
use crate::platform::Platform;
use crate::capability::CapabilityChecker;
use crate::view::DamageRegion;
use super::renderer::{Renderer, DrawingContext, PlaceholderRenderer, CURRENT_BUFFER};
use super::types::RendererBackend;
use super::surface::{Surface, SurfaceInfo};

pub struct Canvas {
    handle: *mut c_void,
    renderer: Option<Box<dyn Renderer>>,
    _backend: RendererBackend,
    width: u32,
    height: u32,
    scale_factor: f32,
    damage: Mutex<DamageRegion>,
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

        let mut renderer: Box<dyn Renderer> = match backend {
            super::types::RendererBackend::Cpu => {
                Box::new(super::cpu::CpuRasterizer::new(width, height))
            }
            _ => {
                Box::new(PlaceholderRenderer::new())
            }
        };
        let surface = Surface::OpenGL { context: std::ptr::null_mut() };
        let surface_info = SurfaceInfo {
            width,
            height,
            scale_factor: 1.0,
        };
        
        renderer.init(surface, surface_info)?;

        let platform = Platform::current();
        let capabilities = CapabilityChecker::new();
        
        let scale_factor = unsafe {
            let window = ng_platform_canvas_get_window(handle);
            if !window.is_null() {
                ng_platform_get_scale_factor(window)
            } else {
                1.0
            }
        };
        
        Ok(Self {
            handle,
            renderer: Some(renderer),
            _backend: backend,
            width,
            height,
            scale_factor,
            damage: Mutex::new(DamageRegion::new(16)),
            platform,
            capabilities,
        })
    }
    
    fn handle_resize(&mut self, new_width: u32, new_height: u32) -> AureaResult<()> {
        if new_width == self.width && new_height == self.height {
            return Ok(());
        }
        
        self.width = new_width;
        self.height = new_height;
        
        let new_scale = unsafe {
            let window = ng_platform_canvas_get_window(self.handle);
            if !window.is_null() {
                ng_platform_get_scale_factor(window)
            } else {
                self.scale_factor
            }
        };
        
        if new_scale != self.scale_factor {
            self.scale_factor = new_scale;
            if let Some(ref mut renderer) = self.renderer {
                let surface = Surface::OpenGL { context: std::ptr::null_mut() };
                let surface_info = SurfaceInfo {
                    width: new_width,
                    height: new_height,
                    scale_factor: new_scale,
                };
                renderer.init(surface, surface_info)?;
            }
        }
        
        CURRENT_BUFFER.with(|buf| {
            *buf.borrow_mut() = None;
        });
        
        if let Some(ref mut renderer) = self.renderer {
            renderer.resize(new_width, new_height)?;
        }
        
        Ok(())
    }

    pub fn draw<F>(&mut self, draw_fn: F) -> AureaResult<()>
    where
        F: FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    {
        self.check_and_resize()?;
        
        if let Some(ref mut renderer) = self.renderer {
            // Get damage region for this frame
            // If no specific damage, mark full canvas (since we're drawing new content)
            let damage_rect = {
                let mut damage = self.damage.lock().unwrap();
                damage.take()
            };
            
            // Set damage in renderer (for partial redraw support)
            renderer.set_damage(damage_rect);
            
            let mut ctx = renderer.begin_frame()?;
            draw_fn(ctx.as_mut())?;
            renderer.end_frame()?;
            self.update_platform_view();
        }
        Ok(())
    }
    
    /// Add damage to the canvas (called when content changes)
    pub fn add_damage(&self, rect: super::Rect) {
        let mut damage = self.damage.lock().unwrap();
        damage.add(rect);
    }
    
    /// Mark the entire canvas as damaged
    pub fn invalidate_all(&self) {
        let mut damage = self.damage.lock().unwrap();
        damage.add_all();
    }
    
    fn check_and_resize(&mut self) -> AureaResult<()> {
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        unsafe {
            ng_platform_canvas_get_size(self.handle, &mut width, &mut height);
        }
        if width > 0 && height > 0 && (width != self.width || height != self.height) {
            self.handle_resize(width, height)?;
        }
        
        let new_scale = unsafe {
            let window = ng_platform_canvas_get_window(self.handle);
            if !window.is_null() {
                ng_platform_get_scale_factor(window)
            } else {
                self.scale_factor
            }
        };
        
        if new_scale != self.scale_factor {
            self.scale_factor = new_scale;
            if let Some(ref mut renderer) = self.renderer {
                let surface = Surface::OpenGL { context: std::ptr::null_mut() };
                let surface_info = SurfaceInfo {
                    width: self.width,
                    height: self.height,
                    scale_factor: new_scale,
                };
                renderer.init(surface, surface_info)?;
            }
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
        self.invalidate_all();
    }
    
    pub fn invalidate_rect(&self, rect: super::Rect) {
        unsafe {
            ng_platform_canvas_invalidate_rect(self.handle, rect.x, rect.y, rect.width, rect.height);
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
    
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }
}

impl Element for Canvas {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
    
    unsafe fn invalidate_platform(&self, rect: Option<super::Rect>) {
        if let Some(r) = rect {
            unsafe {
                ng_platform_canvas_invalidate_rect(self.handle, r.x, r.y, r.width, r.height);
            }
        } else {
            unsafe {
                ng_platform_canvas_invalidate(self.handle);
            }
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        if let Some(ref mut renderer) = self.renderer {
            renderer.cleanup();
        }
    }
}

