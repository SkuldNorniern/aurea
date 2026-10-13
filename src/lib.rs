#[cfg(target_os = "android")]
mod android;

pub use aurea_animation as animation;

pub mod dialog;
mod dispatch;
pub mod elements;
pub mod embed;
pub mod ffi;
pub mod files;
pub mod integration;
pub mod keymap;
pub mod lifecycle;
pub mod locale;
pub mod logger;
pub mod menu;
pub mod platform;
pub mod prelude;
pub mod registry;
pub mod render;
pub mod tray;
pub mod ui;
pub mod window;

pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};

pub use crate::dispatch::on_ui_thread;
pub use crate::elements::{
    Button, Checkbox, ComboBox, Container, Divider, Element, ImageView, Label, Orientation,
    ProgressBar, SidebarList, Slider, Spacer, SplitOrientation, SplitView, Stack, TabBar,
    TextEditor, TextField, TextView,
};
use crate::ffi::ng_platform_reduced_motion;
pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};
pub use crate::tray::TrayIcon;
pub use crate::window::{
    CursorGrabMode, CursorIcon, Window, WindowId, WindowManager, WindowType, clipboard_text,
    set_clipboard_text,
};

pub use crate::window::{
    EventCallback, KeyCode, Modifiers, MouseButton, MouseButtons, PenPhase, PenSample, PenTool,
    WindowEvent,
};

pub use crate::render::{Canvas, Color, DrawingContext, Point, Rect, RendererBackend};

pub use crate::files::{cache_dir, config_dir, data_dir, local_data_dir, open_path, reveal_path};
pub use crate::keymap::{Chord, Keymap};
pub use crate::locale::{locale, locales};
pub use crate::platform::handles::NativeWindowHandle;
pub use aurea_foundation::{AureaError, AureaResult};
pub use aurea_foundation::{
    Capability, CapabilityChecker, DesktopPlatform, MobilePlatform, Platform, Support,
};

/// Whether the user asked the system for less motion: Windows' "animation effects", macOS' "reduce
/// motion", GTK's animation switch, iOS' "reduce motion".
pub fn reduced_motion() -> bool {
    unsafe { ng_platform_reduced_motion() != 0 }
}

/// How well Aurea supports `capability` in this build, on this platform.
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
