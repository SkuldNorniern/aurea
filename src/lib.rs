mod sync {
    pub use aurea_foundation::lock;
}

#[cfg(target_os = "android")]
mod android;

pub mod elements;
pub mod embed;
pub mod ffi;
pub mod integration;
pub mod lifecycle;
pub mod logger;
pub mod menu;
pub mod platform;
pub mod registry;
pub mod render;
pub mod view;
pub mod window;

pub use crate::view::FrameScheduler;
pub use crate::view::damage::DamageRegion;
pub use aurea_runtime::FrameInfo;

pub use crate::elements::{
    Box, BoxOrientation, Container, Divider, Element, Label, Spacer, SplitOrientation, SplitView,
    TextField,
};
pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};
pub use crate::window::{
    CursorGrabMode, Window, WindowId, WindowManager, WindowType, clipboard_text, set_clipboard_text,
};

pub use crate::window::{EventCallback, KeyCode, Modifiers, MouseButton, WindowEvent};

pub use crate::platform::handles::NativeWindowHandle;
pub use aurea_foundation::{AureaError, AureaResult};
pub use aurea_foundation::{
    Capability, CapabilityChecker, DesktopPlatform, MobilePlatform, Platform,
};
