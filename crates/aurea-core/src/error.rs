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
    /// FFI ABI version mismatch between Rust and native library
    AbiVersionMismatch { expected: i32, got: i32 },
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
                let hint = if *code == 1 {
                    #[cfg(target_os = "linux")]
                    { " Linux: install libgtk-3-dev (apt) or gtk3-devel (dnf)." }
                    #[cfg(target_os = "macos")]
                    { " macOS: ensure Xcode command line tools are installed." }
                    #[cfg(target_os = "windows")]
                    { " Windows: ensure MSVC build tools are installed." }
                    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
                    { " Check platform dependencies." }
                } else {
                    " Check platform dependencies."
                };
                write!(f, "Platform error (code {}){}.", code, hint)
            }
            AureaError::EventLoopError => write!(f, "The event loop encountered an error"),
            AureaError::ElementOperationFailed => write!(f, "An operation on a GUI element failed"),
            AureaError::CanvasCreationFailed => write!(f, "Failed to create a canvas"),
            AureaError::RendererInitFailed => write!(f, "Failed to initialize renderer"),
            AureaError::RenderingFailed => write!(f, "Rendering operation failed"),
            AureaError::BackendNotAvailable => {
                write!(f, "Requested rendering backend is not yet implemented")
            }
            AureaError::AbiVersionMismatch { expected, got } => {
                write!(
                    f,
                    "FFI ABI version mismatch: expected {}, got {}",
                    expected, got
                )
            }
        }
    }
}

impl std::error::Error for AureaError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_version_mismatch_display() {
        let e = AureaError::AbiVersionMismatch {
            expected: 1,
            got: 0,
        };
        let s = e.to_string();
        assert!(s.contains("mismatch"));
        assert!(s.contains("expected 1"));
        assert!(s.contains("got 0"));
    }

    #[test]
    fn platform_error_includes_actionable_hint_for_init_failure() {
        let e = AureaError::PlatformError(1);
        let s = e.to_string();
        assert!(s.contains("Platform error"));
        assert!(s.contains("code 1"));
        assert!(
            s.contains("Linux") || s.contains("macOS") || s.contains("Windows") || s.contains("platform dependencies"),
            "display must include platform-specific or fallback hint"
        );
    }
}
