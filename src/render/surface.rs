//! Platform-agnostic surface abstraction

use std::os::raw::c_void;

/// Platform-specific surface types
#[derive(Debug)]
pub enum Surface {
    /// macOS Metal layer
    Metal {
        layer: *mut c_void,
    },
    /// OpenGL context
    OpenGL {
        context: *mut c_void,
    },
    /// Vulkan surface
    Vulkan {
        surface: *mut c_void,
    },
    /// DirectX swap chain
    DirectX {
        swap_chain: *mut c_void,
    },
}

impl Surface {
    /// Get the raw handle for the surface
    pub fn handle(&self) -> *mut c_void {
        match self {
            Surface::Metal { layer } => *layer,
            Surface::OpenGL { context } => *context,
            Surface::Vulkan { surface } => *surface,
            Surface::DirectX { swap_chain } => *swap_chain,
        }
    }
}

/// Surface information
#[derive(Debug, Clone)]
pub struct SurfaceInfo {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

