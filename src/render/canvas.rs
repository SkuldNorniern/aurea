//! Canvas element for custom rendering

use std::os::raw::c_void;

use crate::elements::Element;
use crate::{AureaError, AureaResult};
use crate::ffi::*;
use super::renderer::{Renderer, DrawingContext};
use super::types::RendererBackend;

/// Canvas element that supports custom rendering
pub struct Canvas {
    handle: *mut c_void,
    renderer: Option<Box<dyn Renderer>>,
    _backend: RendererBackend,
    width: u32,
    height: u32,
}

impl Canvas {
    /// Create a new canvas with the specified dimensions and backend
    ///
    /// # Errors
    ///
    /// Returns `AureaError::ElementOperationFailed` if the canvas could not be created
    pub fn new(width: u32, height: u32, backend: RendererBackend) -> AureaResult<Self> {
        let handle = unsafe {
            ng_platform_create_canvas(width as i32, height as i32)
        };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self {
            handle,
            renderer: None,
            _backend: backend,
            width,
            height,
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
        if let Some(ref mut renderer) = self.renderer {
            let mut ctx = renderer.begin_frame()?;
            draw_fn(ctx.as_mut())?;
            renderer.end_frame()?;
        }
        Ok(())
    }

    /// Request a redraw of the canvas
    pub fn invalidate(&self) {
        unsafe {
            ng_platform_canvas_invalidate(self.handle);
        }
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

