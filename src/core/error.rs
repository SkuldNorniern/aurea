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

/// Result type for GUI operations.
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
