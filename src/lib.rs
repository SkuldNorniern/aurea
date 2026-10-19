/// A native GUI toolkit providing cross-platform windowing and widgets with native look and feel.
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
#[cfg(all(any(target_os = "ios", target_os = "android"), not(feature = "mobile")))]
compile_error!(
    "Aurea's iOS and Android hosts are experimental: enable the `mobile` feature to build them"
);

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

pub use crate::dispatch::{after_frame, on_ui_thread};
pub use crate::elements::{
    Button, Checkbox, ComboBox, Container, Divider, Element, ImageView, Label, Orientation,
    ProgressBar, SidebarList, Slider, Spacer, SplitOrientation, SplitView, Stack, TabBar,
    TextEditor, TextField, TextView,
};
use crate::ffi::ng_platform_reduced_motion;
pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};
pub use crate::tray::TrayIcon;
pub use crate::window::clipboard;
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
