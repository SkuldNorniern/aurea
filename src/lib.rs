//! Native GUI toolkit: windowing, widgets and rendering.
//!
//! ```rust,no_run
//! use aurea::prelude::*;
//!
//! fn main() -> AureaResult<()> {
//!     let mut window = Window::new("Hello", 400, 300)?;
//!     window.set_content(Label::new("Hello, Aurea!")?)?;
//!     window.run()?;
//!     Ok(())
//! }
//! ```

// Clippy flags const thread_locals on Android too.
#![cfg_attr(target_os = "android", allow(clippy::missing_const_for_thread_local))]

#[cfg(all(any(target_os = "ios", target_os = "android"), not(feature = "mobile")))]
compile_error!(
    "Aurea's iOS and Android hosts are experimental: enable the `mobile` feature to build them"
);

#[cfg(target_os = "android")]
pub mod android;
#[cfg(all(any(target_os = "ios", target_os = "android"), feature = "zengpu"))]
mod gpu_ready;
#[cfg(target_os = "ios")]
pub mod ios;
#[cfg(any(target_os = "ios", target_os = "android", test))]
mod touch;
#[cfg(any(target_os = "ios", target_os = "android", test))]
mod wake;

pub use aurea_animation as animation;

pub mod app;
pub mod dialog;
mod dispatch;
pub mod elements;
pub mod embed;
/// C ABI entry points for the native layers.
pub mod ffi;
pub mod files;
pub mod integration;
pub mod keymap;
/// App lifecycle notifications.
pub mod lifecycle;
pub mod locale;
/// Logging setup.
pub mod logger;
pub mod menu;
pub mod platform;
pub mod prelude;
/// Callback registries that native code calls back into.
pub mod registry;
pub mod render;
pub mod tray;
pub mod ui;
pub mod window;

pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};

pub use crate::dispatch::{after_frame, on_ui_thread};
pub use crate::elements::{
    Button, Checkbox, ComboBox, Container, Divider, Element, ImageView, Label, Orientation,
    ProgressBar, RowList, ScrollView, ScrollbarPolicy, SidebarList, Slider, Spacer,
    SplitOrientation, SplitView, Stack, TabBar, Table, TextEditor, TextField, TextView,
};
use crate::ffi::{
    ng_platform_beep, ng_platform_prefers_dark, ng_platform_reduced_motion, ng_platform_wheel_lines,
};
pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};
pub use crate::tray::TrayIcon;
pub use crate::window::clipboard;
pub use crate::window::{
    CursorGrabMode, CursorIcon, Window, WindowId, WindowManager, WindowPlacement, WindowProxy,
    WindowType, clipboard_text, set_clipboard_text,
};

pub use crate::window::{
    EventCallback, KeyCode, Modifiers, MouseButton, MouseButtons, Occlusion, PenPhase, PenSample,
    PenTapAction, PenTool, PointerKind, WindowEvent,
};

pub use crate::render::{Canvas, Color, DrawingContext, Point, Rect, RendererBackend};

pub use crate::files::{
    FileCache, Stamp, app_cache_dir, app_config_dir, app_data_dir, app_local_data_dir, cache_dir,
    config_dir, data_dir, documents_dir, local_data_dir, open_path, open_url, reveal_path,
};
pub use crate::keymap::{Chord, Keymap};
pub use crate::locale::{locale, locales};
pub use crate::platform::handles::NativeWindowHandle;
pub use aurea_foundation::{AureaError, AureaResult};
pub use aurea_foundation::{
    Capability, CapabilityChecker, DesktopPlatform, MobilePlatform, OsVersion, Platform, Support,
};

/// Plays the system's alert sound, as a terminal's bell does.
pub fn beep() {
    unsafe { ng_platform_beep() }
}

/// Whether the user asked the system for less motion: Windows' "animation effects", macOS' "reduce
/// motion", GTK's animation switch, iOS' "reduce motion".
pub fn reduced_motion() -> bool {
    unsafe { ng_platform_reduced_motion() != 0 }
}

/// Whether the system appearance is dark, or `None` where Aurea cannot tell yet. A change arrives
/// as [`crate::WindowEvent::ThemeChanged`] on every window.
pub fn prefers_dark() -> Option<bool> {
    match unsafe { ng_platform_prefers_dark() } {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// How far one wheel notch should scroll, from Windows' mouse settings; three lines elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelScroll {
    /// This many lines; 0 turns wheel scrolling off.
    Lines(u32),
    /// One screen at a time.
    Page,
}

/// The user's wheel setting, read each call so a change takes effect at once.
pub fn wheel_scroll() -> WheelScroll {
    match u32::try_from(unsafe { ng_platform_wheel_lines() }) {
        Ok(lines) => WheelScroll::Lines(lines),
        Err(_) => WheelScroll::Page,
    }
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

#[cfg(all(test, target_os = "macos"))]
mod appearance_tests {
    #[test]
    fn macos_always_knows_its_appearance() {
        assert!(super::prefers_dark().is_some());
    }
}

#[cfg(test)]
mod gpu_support_tests {
    use super::*;

    #[test]
    fn gpu_capability_follows_the_compiled_backend() {
        use crate::{Capability, CapabilityChecker, Support};

        let checker = CapabilityChecker::new();
        let narrowed = gpu_support(&checker, Capability::Vulkan);

        if cfg!(any(feature = "zengpu", feature = "wgpu")) {
            assert_eq!(narrowed, checker.support(Capability::Vulkan));
        } else {
            assert!(
                matches!(narrowed, Support::Unimplemented | Support::Unavailable),
                "no GPU backend is compiled in, so it cannot be usable: {narrowed:?}"
            );
        }
    }

    #[test]
    fn other_capabilities_are_not_narrowed() {
        use crate::{Capability, CapabilityChecker};

        let checker = CapabilityChecker::new();
        for capability in [
            Capability::MenuBar,
            Capability::Clipboard,
            Capability::FileDialogs,
            Capability::KeyboardInput,
        ] {
            assert_eq!(
                gpu_support(&checker, capability),
                checker.support(capability),
                "{capability:?} is not a GPU capability"
            );
        }
    }
}
