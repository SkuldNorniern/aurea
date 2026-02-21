use super::traits::Element;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

#[derive(Debug, Clone, Copy)]
pub enum ImageScaling {
    None = 0,
    AspectFit = 1,
    AspectFill = 2,
    Fill = 3,
}

pub struct ImageView {
    handle: *mut c_void,
}

impl ImageView {
    pub fn new() -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_image_view() };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self { handle })
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

    pub fn load_from_path(&mut self, path: &str) -> AureaResult<()> {
        let path = CString::new(path).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe { ng_platform_image_view_load_from_path(self.handle, path.as_ptr()) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    pub fn load_from_data(&mut self, data: &[u8]) -> AureaResult<()> {
        let result = unsafe {
            ng_platform_image_view_load_from_data(self.handle, data.as_ptr(), data.len() as u32)
        };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    pub fn set_scaling(&mut self, scaling: ImageScaling) -> AureaResult<()> {
        unsafe {
            ng_platform_image_view_set_scaling(self.handle, scaling as i32);
        }
        Ok(())
    }
}

impl Element for ImageView {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_image_view_invalidate(self.handle);
        }
    }
}
