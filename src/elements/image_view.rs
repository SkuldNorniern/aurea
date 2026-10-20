use super::native::NativeElement;
use super::traits::Element;
use crate::ffi::c_text;
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::os::raw::c_void;

/// How an image is scaled to fit its view.
#[derive(Debug, Clone, Copy)]
pub enum ImageScaling {
    /// No scaling.
    None = 0,
    /// Scale to fit inside the view, keeping the aspect ratio.
    AspectFit = 1,
    /// Scale to cover the view, keeping the aspect ratio.
    AspectFill = 2,
    /// Stretch to fill the view, ignoring the aspect ratio.
    Fill = 3,
}

/// An element that displays an image.
pub struct ImageView {
    handle: NativeElement,
}

impl ImageView {
    /// Create an empty image view.
    pub fn new() -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_image_view() };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self {
            handle: NativeElement::new(handle),
        })
    }

    /// Create an image view and load an image from a file path.
    pub fn with_path(path: &str) -> AureaResult<Self> {
        let mut image = Self::new()?;
        image.load_from_path(path)?;
        Ok(image)
    }

    /// Create an image view and load raw image bytes.
    pub fn with_data(data: &[u8]) -> AureaResult<Self> {
        let mut image = Self::new()?;
        image.load_from_data(data)?;
        Ok(image)
    }

    /// Create an image view with a scaling mode.
    pub fn with_scaling(scaling: ImageScaling) -> AureaResult<Self> {
        let mut image = Self::new()?;
        image.set_scaling(scaling)?;
        Ok(image)
    }

    /// Load the image at a file path into the view.
    pub fn load_from_path(&mut self, path: &str) -> AureaResult<()> {
        let path = c_text(path)?;
        let result =
            unsafe { ng_platform_image_view_load_from_path(self.handle.handle(), path.as_ptr()) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// Load an image from raw image bytes into the view.
    pub fn load_from_data(&mut self, data: &[u8]) -> AureaResult<()> {
        let result = unsafe {
            ng_platform_image_view_load_from_data(
                self.handle.handle(),
                data.as_ptr(),
                u32::try_from(data.len()).expect("image data fits in u32"),
            )
        };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// Set how the image is scaled to the view.
    pub fn set_scaling(&mut self, scaling: ImageScaling) -> AureaResult<()> {
        unsafe {
            ng_platform_image_view_set_scaling(self.handle.handle(), scaling as i32);
        }
        Ok(())
    }
}

impl Element for ImageView {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {
        unsafe {
            ng_platform_image_view_invalidate(self.handle.handle());
        }
    }
}
