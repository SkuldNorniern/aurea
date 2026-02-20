/// A native GUI toolkit providing cross-platform windowing and widgets with native look and feel.
///
/// ```rust,no_run
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
pub mod core;
pub mod elements;
pub mod events;
pub mod ffi;
pub mod integration;
pub mod lifecycle;
pub mod logger;
pub mod menu;
pub mod platform;
pub mod registry;
pub mod render;
pub mod runtime;
pub mod sync;
pub mod view;
pub mod window;

pub use crate::view::FrameScheduler;
pub use crate::view::damage::DamageRegion;

pub use crate::elements::{
    Box, BoxOrientation, Container, Element, Label, SplitOrientation, SplitView,
};
pub use crate::menu::{MenuBar, SubMenu};
pub use crate::window::{CursorGrabMode, Window, WindowId, WindowManager, WindowType};

pub use crate::window::{EventCallback, KeyCode, Modifiers, MouseButton, WindowEvent};

pub use crate::capability::{Capability, CapabilityChecker};
pub use crate::platform::{DesktopPlatform, MobilePlatform, Platform};

pub use crate::core::{AureaError, AureaResult};
#[cfg(feature = "wgpu")]
pub use crate::integration::NativeWindowHandle;
