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
/// - High-level Rust API (`Window`, `Button`, etc.)
/// - Safe FFI abstractions
/// - Platform-specific native implementations
///
/// # Features
///
/// - Window management
/// - Native menu bars and context menus
/// - Basic widgets (buttons, labels)
/// - Layout management (vertical/horizontal boxes)
/// - Custom rendering with Skia/Vello support (planned)
/// - Event handling
///
/// # Example
///
/// ```rust
/// use aurea::{Window, AureaResult};
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
pub mod lifecycle;
pub mod logger;
pub mod menu;
pub mod platform;
pub mod render;
pub mod view;
pub mod window;

// Re-export the elements, window, and menu modules
pub use crate::elements::{Container, Element};
pub use crate::menu::MenuBar;
pub use crate::window::Window;

// Re-export platform and capability modules
pub use crate::capability::{Capability, CapabilityChecker};
pub use crate::platform::{DesktopPlatform, MobilePlatform, Platform};

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
        }
    }
}

impl std::error::Error for AureaError {}
