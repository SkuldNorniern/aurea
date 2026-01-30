/// A native GUI toolkit providing cross-platform windowing and widgets with native look and feel.
///
/// # Overview
///
/// Aurea is designed to provide a simple, safe, and idiomatic Rust interface to native GUI elements
/// across different platforms. It focuses on providing:
///
/// - Native widgets with platform-specific look and feel
/// - Safe Rust abstractions over platform APIs
/// - Efficient memory management and resource cleanup
/// - Type-safe event handling
///
/// # Architecture
///
/// The library is structured in layers:
/// - **High-level Rust API** (`Window`, `Button`, etc.) - Safe, idiomatic Rust interface
/// - **FFI Layer** (`ffi`) - Safe abstractions over C platform code
/// - **Platform Implementations** - Native C/Objective-C code per platform
///
/// ## Module Organization
///
/// - **`window`** - Window management, events, lifecycle
/// - **`elements`** - UI widgets (Button, Label, Canvas, etc.)
/// - **`render`** - Rendering system (CPU rasterizer, display lists)
/// - **`view`** - View layer (damage tracking, frame scheduling)
/// - **`integration`** - External renderer integrations (wgpu, etc.)
/// - **`platform`** - Platform detection and capabilities
/// - **`lifecycle`** - Application lifecycle events
/// - **`menu`** - Menu bar and menu management
///
/// # Features
///
/// - **Window Management**: Create, manage, and control windows
/// - **Native Widgets**: Platform-native UI elements with native look and feel
/// - **Event System**: Retained-mode event callbacks and non-blocking event polling
/// - **Canvas Rendering**: CPU-first rasterizer with event-driven invalidation
/// - **External Integration**: wgpu surface support for hybrid rendering
/// - **Cross-Platform**: macOS, Windows, Linux, iOS, Android
///
/// # Example
///
/// ```rust
/// use aurea::{Container, Window, AureaResult};
/// use aurea::elements::{Box, BoxOrientation, Button, Label};
///
/// fn main() -> AureaResult<()> {
///     // Create a new window
///     let mut window = Window::new("My App", 800, 600)?;
///     
///     // Create a vertical layout
///     let mut content = Box::new(BoxOrientation::Vertical)?;
///     
///     // Add widgets
///     content.add(Label::new("Welcome!")?)?;
///     content.add(Button::new("Click Me")?)?;
///     
///     // Set window content and run
///     window.set_content(content)?;
///     window.run()?;
///     Ok(())
/// }
/// ```
pub mod capability;
pub mod elements;
pub mod ffi;
pub mod integration;
pub mod lifecycle;
pub mod logger;
pub mod menu;
pub mod platform;
pub mod render;
pub mod view;
pub mod window;

pub use crate::view::FrameScheduler;
pub use crate::view::damage::DamageRegion;

// Re-export the elements, window, and menu modules
pub use crate::elements::{
    Box, BoxOrientation, Container, Element, Label, SplitOrientation, SplitView,
};
pub use crate::menu::{MenuBar, SubMenu};
pub use crate::window::{CursorGrabMode, Window, WindowId, WindowManager, WindowType};

// Re-export window event types
pub use crate::window::{EventCallback, KeyCode, Modifiers, MouseButton, WindowEvent};

// Re-export platform and capability modules
pub use crate::capability::{Capability, CapabilityChecker};
pub use crate::platform::{DesktopPlatform, MobilePlatform, Platform};

// Re-export integration types
#[cfg(feature = "wgpu")]
pub use crate::integration::NativeWindowHandle;

/// Errors that might occur during native GUI operations.
#[derive(Debug, Clone)]
pub enum AureaError {
    /// Failed to create a new window
    WindowCreationFailed,
    /// Failed to create a menu
    MenuCreationFailed,
    /// Failed to add a menu item
    MenuItemAddFailed,
    /// The provided title contains invalid characters
    InvalidTitle,
    /// A platform-specific error occurred
    PlatformError(i32),
    /// The event loop encountered an error
    EventLoopError,
    /// An operation on a GUI element failed
    ElementOperationFailed,
    /// Failed to create a canvas
    CanvasCreationFailed,
    /// Failed to initialize renderer
    RendererInitFailed,
    /// Rendering operation failed
    RenderingFailed,
    /// Requested backend (e.g. Gpu) is not yet implemented
    BackendNotAvailable,
}

/// Result type for GUI operations
pub type AureaResult<T> = std::result::Result<T, AureaError>;

impl std::fmt::Display for AureaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AureaError::WindowCreationFailed => write!(f, "Failed to create a new window"),
            AureaError::MenuCreationFailed => write!(f, "Failed to create a menu"),
            AureaError::MenuItemAddFailed => write!(f, "Failed to add a menu item"),
            AureaError::InvalidTitle => write!(f, "The provided title contains invalid characters"),
            AureaError::PlatformError(code) => {
                write!(f, "A platform-specific error occurred: {}", code)
            }
            AureaError::EventLoopError => write!(f, "The event loop encountered an error"),
            AureaError::ElementOperationFailed => write!(f, "An operation on a GUI element failed"),
            AureaError::CanvasCreationFailed => write!(f, "Failed to create a canvas"),
            AureaError::RendererInitFailed => write!(f, "Failed to initialize renderer"),
            AureaError::RenderingFailed => write!(f, "Rendering operation failed"),
            AureaError::BackendNotAvailable => {
                write!(f, "Requested rendering backend is not yet implemented")
            }
        }
    }
}

impl std::error::Error for AureaError {}
