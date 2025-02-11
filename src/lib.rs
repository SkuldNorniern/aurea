pub mod elements;
pub mod window;
pub mod menu;
pub mod ffi;

// Re-export the elements, window, and menu modules
pub use crate::elements::Element;
pub use crate::window::Window;
pub use crate::menu::MenuBar;

/// Errors that might occur during native GUI operations.
#[derive(Debug)]
pub enum Error {
    WindowCreationFailed,
    MenuCreationFailed,
    MenuItemAddFailed,
    InvalidTitle,
    PlatformError(i32),
    EventLoopError,
    ElementOperationFailed,
}

type Result<T> = std::result::Result<T, Error>;

