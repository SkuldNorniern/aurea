//! Canvas element for custom rendering

use std::os::raw::c_void;

use crate::elements::Element;
use crate::{AureaError, AureaResult};
use crate::ffi::*;
use crate::platform::Platform;
use crate::capability::{Capability, CapabilityChecker};
use super::renderer::{Renderer, DrawingContext, PlaceholderRenderer};
use super::types::RendererBackend;
use super::surface::{Surface, SurfaceInfo};
use log::{debug, info, warn, error};

/// Canvas element that supports custom rendering
pub struct Canvas {
    handle: *mut c_void,
    renderer: Option<Box<dyn Renderer>>,
    _backend: RendererBackend,
    width: u32,
    height: u32,
    platform: Platform,
    capabilities: CapabilityChecker,
}

impl Canvas {
    /// Create a new canvas with the specified dimensions and backend
    ///
    /// # Errors
    ///
    /// Returns `AureaError::ElementOperationFailed` if the canvas could not be created
    pub fn new(width: u32, height: u32, backend: RendererBackend) -> AureaResult<Self> {
        info!("Creating canvas: {}x{} with backend {:?}", width, height, backend);
        
        let handle = unsafe {
            ng_platform_create_canvas(width as i32, height as i32)
        };

        if handle.is_null() {
            error!("Failed to create platform canvas");
            return Err(AureaError::ElementOperationFailed);
        }

        debug!("Platform canvas handle created: {:p}", handle);

        // Initialize placeholder renderer for now
        // TODO: Replace with actual Skia/Vello renderer when implemented
        let mut renderer: Box<dyn Renderer> = Box::new(PlaceholderRenderer::new());
        
        // Create a dummy surface for initialization
        // In a real implementation, we'd get the actual surface from the platform
        let surface = Surface::OpenGL { context: std::ptr::null_mut() };
        let surface_info = SurfaceInfo {
            width,
            height,
            scale_factor: 1.0,
        };
        
        info!("Initializing renderer with surface info: {}x{}", width, height);
        if let Err(e) = renderer.init(surface, surface_info) {
            error!("Failed to initialize renderer: {:?}", e);
            return Err(e);
        }

        let platform = Platform::current();
        let capabilities = CapabilityChecker::new();
        
        if capabilities.has(Capability::HardwareAcceleration) {
            debug!("Platform supports hardware acceleration for canvas rendering");
        }
        
        info!("Canvas created successfully on {}", platform);
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

    /// Draw on the canvas using a closure
    ///
    /// # Errors
    ///
    /// Returns an error if drawing operations fail
    pub fn draw<F>(&mut self, draw_fn: F) -> AureaResult<()>
    where
        F: FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    {
        debug!("Canvas::draw called");
        if let Some(ref mut renderer) = self.renderer {
            debug!("Beginning frame");
            let mut ctx = renderer.begin_frame()?;
            debug!("Calling draw function");
            draw_fn(ctx.as_mut())?;
            debug!("Ending frame");
            renderer.end_frame()?;
            
            // Update the platform view with the rendered buffer
            debug!("Updating platform view");
            self.update_platform_view();
        } else {
            warn!("Canvas draw called but renderer is None");
        }
        Ok(())
    }

    /// Update the platform view with the current render buffer
    fn update_platform_view(&self) {
        debug!("update_platform_view called");
        if let Some((ptr, size, width, height)) = self.get_render_buffer() {
            debug!("Got buffer from renderer: ptr={:p}, size={}, {}x{}", ptr, size, width, height);
            if !ptr.is_null() && size > 0 {
                info!("Updating platform canvas buffer: {}x{}, {} bytes", width, height, size);
                unsafe {
                    ng_platform_canvas_update_buffer(
                        self.handle,
                        ptr,
                        size as u32,
                        width,
                        height,
                    );
                }
            } else {
                warn!("Buffer pointer is null or size is 0");
            }
        } else {
            warn!("No buffer available from renderer");
        }
    }

    /// Request a redraw of the canvas
    pub fn invalidate(&self) {
        debug!("Canvas invalidate called");
        unsafe {
            ng_platform_canvas_invalidate(self.handle);
        }
    }

    /// Get the renderer buffer for platform-specific blitting
    /// Returns (pointer, size_in_bytes, width, height)
    fn get_render_buffer(&self) -> Option<(*const u8, usize, u32, u32)> {
        // Use thread-local storage to get the current renderer's buffer
        // This is set during end_frame()
        use crate::render::renderer::CURRENT_BUFFER;
        CURRENT_BUFFER.with(|buf| {
            *buf.borrow()
        })
    }

    /// Get the canvas width
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get the canvas height
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
        // Native view lifecycle managed by parent container
    }
}

