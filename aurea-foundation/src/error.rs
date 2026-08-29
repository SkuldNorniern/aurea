use crate::platform::Platform;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::result::Result as StdResult;

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
    /// Rendering operation failed
    RenderingFailed,
    /// FFI ABI version mismatch between Rust and native library
    AbiVersionMismatch { expected: i32, got: i32 },
    /// The platform backend has no implementation for this operation.
    ///
    /// Distinct from a failure: nothing went wrong, there is simply no code
    /// for it on this platform. Worth telling apart, because an application
    /// can fall back on one and not on the other.
    Unsupported {
        /// What was attempted, e.g. `"attach_menu"`.
        operation: &'static str,
        /// Where it was attempted.
        platform: Platform,
    },
    /// The native UI has to be brought up on the process main thread, and the
    /// thread asking for it is not that one.
    ///
    /// AppKit and UIKit require it. The other backends are content with any
    /// one thread owning the UI, so this only arises on Apple targets.
    NotMainThread,
}

/// Result type for GUI operations.
pub type AureaResult<T> = StdResult<T, AureaError>;

impl Display for AureaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            AureaError::WindowCreationFailed => write!(f, "Failed to create a new window"),
            AureaError::MenuCreationFailed => write!(f, "Failed to create a menu"),
            AureaError::MenuItemAddFailed => write!(f, "Failed to add a menu item"),
            AureaError::InvalidTitle => write!(f, "The provided title contains invalid characters"),
            AureaError::PlatformError(code) => {
                let hint = if *code == 1 {
                    #[cfg(target_os = "linux")]
                    {
                        " Linux: install libgtk-3-dev (apt) or gtk3-devel (dnf)."
                    }
                    #[cfg(target_os = "macos")]
                    {
                        " macOS: ensure Xcode command line tools are installed."
                    }
                    #[cfg(target_os = "windows")]
                    {
                        " Windows: ensure MSVC build tools are installed."
                    }
                    #[cfg(not(any(
                        target_os = "linux",
                        target_os = "macos",
                        target_os = "windows"
                    )))]
                    {
                        " Check platform dependencies."
                    }
                } else {
                    " Check platform dependencies."
                };
                write!(f, "Platform error (code {}){}.", code, hint)
            }
            AureaError::EventLoopError => write!(f, "The event loop encountered an error"),
            AureaError::NotMainThread => write!(
                f,
                "The native UI must be created on the process main thread on this platform."
            ),
            AureaError::ElementOperationFailed => write!(f, "An operation on a GUI element failed"),
            AureaError::RenderingFailed => write!(f, "Rendering operation failed"),
            AureaError::Unsupported {
                operation,
                platform,
            } => write!(f, "{operation} is not implemented on {platform:?}"),
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

impl Error for AureaError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::MobilePlatform;

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
            s.contains("Linux")
                || s.contains("macOS")
                || s.contains("Windows")
                || s.contains("platform dependencies"),
            "display must include platform-specific or fallback hint"
        );
    }
    #[test]
    fn unsupported_names_the_operation_and_the_platform() {
        let err = AureaError::Unsupported {
            operation: "attach_menu",
            platform: Platform::Mobile(MobilePlatform::Android),
        };
        let text = err.to_string();

        assert!(text.contains("attach_menu"), "got {text}");
        assert!(text.contains("Android"), "got {text}");
    }

    /// Not the same thing as a failure: nothing went wrong.
    #[test]
    fn unsupported_is_its_own_error() {
        let unsupported = AureaError::Unsupported {
            operation: "x",
            platform: Platform::Mobile(MobilePlatform::IOS),
        };
        assert!(!matches!(unsupported, AureaError::ElementOperationFailed));
    }
}
