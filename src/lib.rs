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
/// - **`platform`** - Native window/display handle extraction and conversion
/// - **`integration`** - External renderer integrations (wgpu, etc.)
/// - **`lifecycle`** - Application lifecycle events
/// - **`menu`** - Menu bar and menu management
/// - **`prelude`** - Glob-importable set of the most commonly used types
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
/// Interior mutability uses `Mutex`; `aurea_foundation::lock` is used throughout because we do not
/// panic while holding a lock, so the mutex is never poisoned.
///
/// # Example
///
/// ```rust,no_run
/// use aurea::prelude::*;
///
/// fn main() -> AureaResult<()> {
///     let mut window = Window::new("Hello", 400, 300)?;
///     window.set_content(Label::new("Hello, Aurea!")?)?;
///     window.run()?;
///     Ok(())
/// }
/// ```
#[cfg(target_os = "android")]
mod android;

pub use aurea_animation as animation;

pub mod elements;
pub mod embed;
pub mod ffi;
pub mod integration;
pub mod lifecycle;
pub mod logger;
pub mod menu;
pub mod platform;
pub mod prelude;
pub mod registry;
pub mod render;
pub mod window;

pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};

// Re-export the elements, window, and menu modules
pub use crate::elements::{
    Button, Checkbox, ComboBox, Container, Divider, Element, ImageView, Label, Orientation,
    ProgressBar, SidebarList, Slider, Spacer, SplitOrientation, SplitView, Stack, TabBar,
    TextEditor, TextField, TextView,
};
pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};
pub use crate::window::{
    CursorGrabMode, Window, WindowId, WindowManager, WindowType, clipboard_text, set_clipboard_text,
};

// Re-export window event types
pub use crate::window::{EventCallback, KeyCode, Modifiers, MouseButton, WindowEvent};

// Re-export the canvas rendering surface and its core drawing types
pub use crate::render::{Canvas, Color, DrawingContext, Point, Rect, RendererBackend};

pub use crate::platform::handles::NativeWindowHandle;
pub use aurea_foundation::{AureaError, AureaResult};
pub use aurea_foundation::{
    Capability, CapabilityChecker, DesktopPlatform, MobilePlatform, Platform, Support,
};

/// How well Aurea supports `capability` in this build, on this platform.
///
/// [`CapabilityChecker::support`] answers for the platform alone, because
/// `aurea-foundation` knows nothing about this crate's Cargo features. GPU
/// backends are compiled in or they are not, and a build without one cannot
/// use them however capable the hardware is — so this narrows that answer.
///
/// ```rust
/// use aurea::{Capability, CapabilityChecker, Support};
///
/// let checker = CapabilityChecker::new();
/// // Without the `zengpu` or `wgpu` feature this reports Unimplemented, even
/// // on a machine with a perfectly good GPU.
/// let _ = aurea::gpu_support(&checker, Capability::Vulkan);
/// # let _ = Support::Unimplemented;
/// ```
pub fn gpu_support(checker: &CapabilityChecker, capability: Capability) -> Support {
    let platform_support = checker.support(capability);
    if !CapabilityChecker::is_gpu(capability) {
        return platform_support;
    }
    if platform_support == Support::Unavailable {
        return platform_support;
    }

    let compiled_in = cfg!(feature = "zengpu") || cfg!(feature = "wgpu");
    if compiled_in {
        platform_support
    } else {
        Support::Unimplemented
    }
}
