//! Platform-agnostic surface abstraction.

use std::os::raw::c_void;
use std::ptr::null_mut;

use crate::numeric::f32_to_u32_clamped;

/// Platform-specific surface types.
#[derive(Debug)]
pub enum Surface {
    /// CPU rasterizer target — no native surface object.
    Cpu,
    /// macOS Metal layer.
    Metal {
        /// `CAMetalLayer` pointer.
        layer: *mut c_void,
    },
    /// Vulkan surface.
    Vulkan {
        /// `VkSurfaceKHR` handle.
        surface: *mut c_void,
    },
    /// DirectX swap chain.
    DirectX {
        /// `IDXGISwapChain` pointer.
        swap_chain: *mut c_void,
    },
}

impl Surface {
    /// Get the raw handle for the surface.
    pub fn handle(&self) -> *mut c_void {
        match self {
            Surface::Cpu => null_mut(),
            Surface::Metal { layer } => *layer,
            Surface::Vulkan { surface } => *surface,
            Surface::DirectX { swap_chain } => *swap_chain,
        }
    }
}

/// Surface information.
#[derive(Debug, Clone)]
pub struct SurfaceInfo {
    /// Logical size.
    pub width: u32,
    /// Logical height.
    pub height: u32,
    /// Logical to device pixel scale.
    pub scale_factor: f32,
    /// The size in device pixels, when the platform knows it.
    pub pixels: Option<(u32, u32)>,
}

impl SurfaceInfo {
    /// A surface `width` by `height` logical pixels at `scale_factor`.
    pub fn new(width: u32, height: u32, scale_factor: f32) -> Self {
        Self {
            width,
            height,
            scale_factor,
            pixels: None,
        }
    }

    /// The same surface, known to be `width` by `height` device pixels.
    #[must_use]
    pub fn with_pixels(mut self, width: u32, height: u32) -> Self {
        self.pixels = Some((width, height));
        self
    }

    /// The size in device pixels: [`Self::pixels`] when it fits the logical size (within a logical
    /// pixel of it times the scale), else the logical size times the scale.
    pub fn pixel_size(&self) -> (u32, u32) {
        pixel_size(self.width, self.height, self.scale_factor, self.pixels)
    }
}

pub(crate) fn pixel_size(
    width: u32,
    height: u32,
    scale_factor: f32,
    pixels: Option<(u32, u32)>,
) -> (u32, u32) {
    let s = scale_factor.max(1.0);
    let scaled = |logical: u32| logical as f32 * s;
    let fits =
        |device: u32, logical: u32| device > 0 && (device as f32 - scaled(logical)).abs() <= s;
    if let Some((pw, ph)) = pixels
        && fits(pw, width)
        && fits(ph, height)
    {
        return (pw, ph);
    }
    (
        f32_to_u32_clamped(scaled(width).round()).max(1),
        f32_to_u32_clamped(scaled(height).round()).max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_pixels_win_when_they_fit() {
        let info = SurfaceInfo::new(668, 400, 1.5).with_pixels(1001, 600);
        assert_eq!(info.pixel_size(), (1001, 600));
        assert_eq!(SurfaceInfo::new(668, 400, 1.5).pixel_size(), (1002, 600));
        let stale = SurfaceInfo::new(668, 400, 1.5).with_pixels(500, 600);
        assert_eq!(stale.pixel_size(), (1002, 600));
        assert_eq!(SurfaceInfo::new(0, 0, 2.0).pixel_size(), (1, 1));
    }
}
