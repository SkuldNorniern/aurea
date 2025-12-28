use std::os::raw::c_void;
use std::sync::{Mutex, Arc};
use crate::elements::Element;
use crate::{AureaError, AureaResult};
use crate::ffi::*;
use crate::platform::Platform;
use crate::capability::CapabilityChecker;
use crate::view::{DamageRegion, FrameScheduler};
use super::renderer::{Renderer, DrawingContext, PlaceholderRenderer, CURRENT_BUFFER};
use super::types::{RendererBackend, Color};
use super::surface::{Surface, SurfaceInfo};

/// Drawing callback type - stored and called when canvas needs redraw
pub type DrawCallback = Box<dyn Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync>;

pub struct Canvas {
    handle: *mut c_void,
    renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
    _backend: RendererBackend,
    width: u32,
    height: u32,
    scale_factor: f32,
    damage: Arc<Mutex<DamageRegion>>,
    // Retained-mode properties
    background_color: Arc<Mutex<Color>>,
    draw_callback: Arc<Mutex<Option<DrawCallback>>>,
    needs_redraw: Arc<Mutex<bool>>,
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
        
        let renderer_arc = Arc::new(Mutex::new(Some(renderer)));
        let damage_arc = Arc::new(Mutex::new(DamageRegion::new(16)));
        let bg_color_arc = Arc::new(Mutex::new(Color::rgb(255, 255, 255)));
        let draw_cb_arc = Arc::new(Mutex::new(None));
        let needs_redraw_arc = Arc::new(Mutex::new(false));
        
        // Clone Arcs for callback
        let renderer_for_callback = renderer_arc.clone();
        let damage_for_callback = damage_arc.clone();
        let bg_color_for_callback = bg_color_arc.clone();
        let draw_cb_for_callback = draw_cb_arc.clone();
        let needs_redraw_for_callback = needs_redraw_arc.clone();
        
        let canvas = Self {
            handle,
            renderer: renderer_arc,
            _backend: backend,
            width,
            height,
            scale_factor,
            damage: damage_arc,
            background_color: bg_color_arc,
            draw_callback: draw_cb_arc,
            needs_redraw: needs_redraw_arc,
            platform,
            capabilities,
        };
        
        // Register with frame scheduler for automatic redraw
        canvas.register_with_scheduler(
            renderer_for_callback,
            damage_for_callback,
            bg_color_for_callback,
            draw_cb_for_callback,
            needs_redraw_for_callback,
        );
        
        Ok(canvas)
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
        {
            let mut renderer_guard = self.renderer.lock().unwrap();
            if let Some(ref mut renderer) = *renderer_guard {
                let surface = Surface::OpenGL { context: std::ptr::null_mut() };
                let surface_info = SurfaceInfo {
                    width: new_width,
                    height: new_height,
                    scale_factor: new_scale,
                };
                renderer.init(surface, surface_info)?;
            }
        }
        }
        
        CURRENT_BUFFER.with(|buf| {
            *buf.borrow_mut() = None;
        });
        
        {
            let mut renderer_guard = self.renderer.lock().unwrap();
            if let Some(ref mut renderer) = *renderer_guard {
                renderer.resize(new_width, new_height)?;
            }
        }
        
        Ok(())
    }

    /// Set the drawing callback (retained-mode style)
    /// The callback will be called automatically when the canvas needs redraw
    pub fn set_draw_callback<F>(&self, callback: F) -> AureaResult<()>
    where
        F: Fn(&mut dyn DrawingContext) -> AureaResult<()> + Send + Sync + 'static,
    {
        let mut cb = self.draw_callback.lock().unwrap();
        *cb = Some(Box::new(callback));
        
        // Mark as needing redraw
        *self.needs_redraw.lock().unwrap() = true;
        self.invalidate_all();
        
        Ok(())
    }
    
    /// Draw immediately (legacy API - still supported)
    /// Prefer using `set_draw_callback()` for retained-mode style
    pub fn draw<F>(&mut self, draw_fn: F) -> AureaResult<()>
    where
        F: FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    {
        // For FnOnce, we can't store it, so we execute immediately
        // This maintains backward compatibility
        self.check_and_resize()?;
        
        {
            let mut renderer_guard = self.renderer.lock().unwrap();
            if let Some(ref mut renderer) = *renderer_guard {
                // Get damage region for this frame
                let damage_rect = {
                    let mut damage = self.damage.lock().unwrap();
                    damage.take()
                };
                
                // Set damage in renderer (for partial redraw support)
                renderer.set_damage(damage_rect);
                
                let mut ctx = renderer.begin_frame()?;
                
                // Clear with background color
                let bg_color = *self.background_color.lock().unwrap();
                ctx.clear(bg_color)?;
                
                // Execute the draw function
                draw_fn(ctx.as_mut())?;
                
                renderer.end_frame()?;
            }
        }
        
        // Update platform view after releasing renderer lock
        self.update_platform_view();
        
        Ok(())
    }
    
    /// Internal: Redraw if needed (called automatically when invalidated)
    /// This performs the actual drawing using the stored callback
    /// Note: This is now handled by the frame scheduler callback
    /// Kept for backward compatibility if called directly
    fn perform_redraw(&mut self) -> AureaResult<()> {
        self.check_and_resize()?;
        
        // Get callback reference (we can't clone, so we'll borrow)
        let has_callback = {
            let cb = self.draw_callback.lock().unwrap();
            cb.is_some()
        };
        
        if !has_callback {
            return Ok(());
        }
        
        {
            let mut renderer_guard = self.renderer.lock().unwrap();
            if let Some(ref mut renderer) = *renderer_guard {
                // Get damage region for this frame
                let damage_rect = {
                    let mut damage = self.damage.lock().unwrap();
                    damage.take()
                };
                
                // Set damage in renderer (for partial redraw support)
                renderer.set_damage(damage_rect);
                
                let mut ctx = renderer.begin_frame()?;
                
                // Clear with background color
                let bg_color = *self.background_color.lock().unwrap();
                ctx.clear(bg_color)?;
                
                // Call stored callback
                {
                    let cb = self.draw_callback.lock().unwrap();
                    if let Some(ref callback) = *cb {
                        callback(ctx.as_mut())?;
                    }
                }
                
                renderer.end_frame()?;
            }
        }
        
        // Update platform view after releasing renderer lock
        self.update_platform_view();
        
        Ok(())
    }
    
    /// Set background color (property setter - marks damage automatically)
    pub fn set_background_color(&self, color: Color) {
        let mut bg = self.background_color.lock().unwrap();
        if *bg != color {
            *bg = color;
            self.invalidate_all();
        }
    }
    
    /// Get background color
    pub fn background_color(&self) -> Color {
        *self.background_color.lock().unwrap()
    }
    
    /// Add damage to the canvas (called when content changes)
    pub fn add_damage(&self, rect: super::Rect) {
        let mut damage = self.damage.lock().unwrap();
        damage.add(rect);
    }
    
    /// Mark the entire canvas as damaged
    /// This automatically schedules a redraw using the stored callback
    pub fn invalidate_all(&self) {
        let mut damage = self.damage.lock().unwrap();
        damage.add_all();
        
        // Schedule frame for redraw
        FrameScheduler::schedule();
        
        // Mark that we need to redraw
        *self.needs_redraw.lock().unwrap() = true;
        
        // Invalidate platform view
        unsafe {
            ng_platform_canvas_invalidate(self.handle);
        }
    }
    
    /// Check if canvas needs redraw and perform it
    /// This should be called from the frame scheduler or window's frame handler
    pub fn redraw_if_needed(&mut self) -> AureaResult<()> {
        let needs_redraw = {
            let mut flag = self.needs_redraw.lock().unwrap();
            if !*flag {
                return Ok(());
            }
            *flag = false;
            true
        };
        
        if needs_redraw {
            self.perform_redraw()?;
        }
        
        Ok(())
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
            {
                let mut renderer_guard = self.renderer.lock().unwrap();
                if let Some(ref mut renderer) = *renderer_guard {
                    let surface = Surface::OpenGL { context: std::ptr::null_mut() };
                    let surface_info = SurfaceInfo {
                        width: self.width,
                        height: self.height,
                        scale_factor: new_scale,
                    };
                    renderer.init(surface, surface_info)?;
                }
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
    
    /// Invalidate a specific rectangle (property setter style - marks damage automatically)
    pub fn invalidate_rect(&self, rect: super::Rect) {
        // Add to damage region
        self.add_damage(rect);
        
        // Schedule frame
        FrameScheduler::schedule();
        
        // Mark needs redraw
        *self.needs_redraw.lock().unwrap() = true;
        
        // Invalidate platform view
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
    
    /// Register this canvas with the frame scheduler
    /// This enables automatic redraw when frames are scheduled
    fn register_with_scheduler(
        &self,
        renderer: Arc<Mutex<Option<Box<dyn Renderer>>>>,
        damage: Arc<Mutex<DamageRegion>>,
        background_color: Arc<Mutex<Color>>,
        draw_callback: Arc<Mutex<Option<DrawCallback>>>,
        needs_redraw: Arc<Mutex<bool>>,
    ) {
        // Convert handle to usize for thread safety
        let handle_usize = self.handle as usize;
        
        // Create a callback that can perform redraw using interior mutability
        let callback: Arc<dyn Fn() -> AureaResult<()> + Send + Sync> = Arc::new(move || {
            // Check if redraw is needed
            let should_redraw = {
                let mut flag = needs_redraw.lock().unwrap();
                if !*flag {
                    return Ok(());
                }
                *flag = false;
                true
            };
            
            if !should_redraw {
                return Ok(());
            }
            
            // Get damage region
            let damage_rect = {
                let mut d = damage.lock().unwrap();
                d.take()
            };
            
            // Get renderer
            let mut renderer_guard = renderer.lock().unwrap();
            if let Some(ref mut r) = *renderer_guard {
                // Set damage
                r.set_damage(damage_rect);
                
                // Begin frame
                let mut ctx = r.begin_frame()?;
                
                // Clear with background color
                let bg_color = *background_color.lock().unwrap();
                ctx.clear(bg_color)?;
                
                // Call stored callback
                {
                    let cb = draw_callback.lock().unwrap();
                    if let Some(ref callback_fn) = *cb {
                        callback_fn(ctx.as_mut())?;
                    }
                }
                
                // End frame
                r.end_frame()?;
                
                // Update platform view (get buffer and update)
                use crate::render::renderer::CURRENT_BUFFER;
                if let Some((ptr, size, w, h)) = CURRENT_BUFFER.with(|buf| *buf.borrow()) {
                    if !ptr.is_null() && size > 0 {
                        unsafe {
                            let handle_ptr = handle_usize as *mut c_void;
                            ng_platform_canvas_update_buffer(handle_ptr, ptr, size as u32, w, h);
                        }
                    }
                }
            }
            
            Ok(())
        });
        
        FrameScheduler::register_canvas(self.handle, callback);
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
        // Unregister from frame scheduler
        FrameScheduler::unregister_canvas(self.handle);
        
        // Cleanup renderer
        let mut renderer_guard = self.renderer.lock().unwrap();
        if let Some(ref mut renderer) = *renderer_guard {
            renderer.cleanup();
        }
    }
}

