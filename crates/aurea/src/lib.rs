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
/// # Implementation note
///
/// Interior mutability uses `Mutex`; `aurea_core::lock` is used throughout because we do not
/// panic while holding a lock, so the mutex is never poisoned.
///
/// # Example
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
mod sync {
    pub use aurea_core::lock;
}

pub mod elements;
pub mod ffi;
pub mod integration;
pub mod lifecycle;
pub mod logger;
pub mod menu;
pub mod registry;
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

pub use aurea_core::{Capability, CapabilityChecker, DesktopPlatform, MobilePlatform, Platform};
pub use aurea_core::{AureaError, AureaResult};
#[cfg(feature = "wgpu")]
pub use crate::integration::NativeWindowHandle;
