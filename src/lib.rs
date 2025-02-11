/// A native GUI toolkit providing cross-platform windowing and widgets.
/// 
/// # Features
/// 
/// - Native window creation and management
/// - Menu bar and menu items
/// - Basic widgets (buttons, labels, boxes)
/// - Layout management
/// 
/// # Example
/// 
/// ```rust
/// use fenestra::{Window, Error};
/// use fenestra::elements::{Box, BoxOrientation, Button, Label};
/// 
/// fn main() -> Result<(), Error> {
///     let mut window = Window::new("My App", 800, 600)?;
///     
///     let mut content = Box::new(BoxOrientation::Vertical)?;
///     let label = Label::new("Hello, World!")?;
///     let button = Button::new("Click Me")?;
///     
///     content.add(label)?;
///     content.add(button)?;
///     
///     window.set_content(content)?;
///     window.run()?;
///     Ok(())
/// }
/// ```
pub mod elements;
pub mod window;
pub mod menu;
pub mod ffi;

// Re-export the elements, window, and menu modules
pub use crate::elements::Element;
pub use crate::window::Window;
pub use crate::menu::MenuBar;

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
            AureaError::PlatformError(code) => write!(f, "A platform-specific error occurred: {}", code),
            AureaError::EventLoopError => write!(f, "The event loop encountered an error"),
            AureaError::ElementOperationFailed => write!(f, "An operation on a GUI element failed"),
        }
    }
}

impl std::error::Error for AureaError {}

