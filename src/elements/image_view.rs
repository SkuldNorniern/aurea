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
                u32::try_from(data.len()).map_err(|_| AureaError::ElementOperationFailed)?,
            )
        };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// The shown image's size in pixels, upright after any EXIF rotation, or `None` when empty.
    pub fn image_size(&self) -> Option<(u32, u32)> {
        let (mut width, mut height) = (0, 0);
        let result = unsafe {
            ng_platform_image_view_get_size(self.handle.handle(), &mut width, &mut height)
        };
        (result == 0).then_some((width, height))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Window;

    const PNG_3X2: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 3, 0, 0, 0, 2, 8, 2,
        0, 0, 0, 18, 22, 241, 77, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 96, 192, 4, 0, 0, 20,
        0, 1, 238, 90, 105, 9, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    const JPEG_4X2_TURNED: &[u8] = &[
        255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 225, 0, 34,
        69, 120, 105, 102, 0, 0, 77, 77, 0, 42, 0, 0, 0, 8, 0, 1, 1, 18, 0, 3, 0, 0, 0, 1, 0, 6, 0,
        0, 0, 0, 0, 0, 255, 219, 0, 67, 0, 16, 11, 12, 14, 12, 10, 16, 14, 13, 14, 18, 17, 16, 19,
        24, 40, 26, 24, 22, 22, 24, 49, 35, 37, 29, 40, 58, 51, 61, 60, 57, 51, 56, 55, 64, 72, 92,
        78, 64, 68, 87, 69, 55, 56, 80, 109, 81, 87, 95, 98, 103, 104, 103, 62, 77, 113, 121, 112,
        100, 120, 92, 101, 103, 99, 255, 219, 0, 67, 1, 17, 18, 18, 24, 21, 24, 47, 26, 26, 47, 99,
        66, 56, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
        99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
        99, 99, 99, 99, 99, 99, 99, 255, 192, 0, 17, 8, 0, 2, 0, 4, 3, 1, 34, 0, 2, 17, 1, 3, 17,
        1, 255, 196, 0, 21, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 255, 196, 0,
        20, 16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 196, 0, 21, 1, 1, 1, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 6, 255, 196, 0, 20, 17, 1, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 255, 218, 0, 12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 142, 0, 42, 135,
        255, 217,
    ];

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn images_report_their_upright_size() -> AureaResult<()> {
        let _window = Window::new("image", 200, 100)?;
        let mut view = ImageView::new()?;
        assert_eq!(view.image_size(), None);
        view.load_from_data(PNG_3X2)?;
        assert_eq!(view.image_size(), Some((3, 2)));
        view.load_from_data(JPEG_4X2_TURNED)?;
        assert_eq!(view.image_size(), Some((2, 4)));
        assert!(view.load_from_data(b"not an image").is_err());
        assert_eq!(view.image_size(), Some((2, 4)));
        Ok(())
    }
}
