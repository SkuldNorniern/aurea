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

use std::sync::atomic::{AtomicBool, Ordering};

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
mod keyboard;
pub mod keymap;
/// App lifecycle notifications.
pub mod lifecycle;
pub mod locale;
/// Logging setup.
pub mod logger;
pub mod menu;
mod pen;
pub mod platform;
pub mod prelude;
/// Callback registries that native code calls back into.
pub mod registry;
pub mod render;
mod screen;
mod sheets;
pub mod tray;
pub mod ui;
pub mod window;

pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};

pub use crate::dispatch::{
    LateWakeHandle, WakeHandle, after_frame, on_ui_thread, run_after, wake_canvas, wake_handle,
};
pub use crate::elements::{
    Button, Checkbox, ComboBox, Container, Divider, Element, ImageView, Label, Orientation,
    ProgressBar, RadioGroup, RowList, ScrollView, ScrollbarPolicy, SidebarList, Slider, Spacer,
    SpinBox, SplitOrientation, SplitView, Stack, TabBar, Table, TextEditor, TextField, TextView,
};
use crate::ffi::{
    ng_platform_beep, ng_platform_prefers_dark, ng_platform_reduced_motion, ng_platform_wheel_lines,
};
pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};
pub use crate::tray::TrayIcon;
pub use crate::window::clipboard;
pub use crate::window::{
    CloseAnswer, CursorGrabMode, CursorIcon, Window, WindowId, WindowManager, WindowPlacement,
    WindowProxy, WindowType, clipboard_text, set_clipboard_text,
};

pub use crate::window::{
    EventCallback, GesturePhase, KeyCode, Modifiers, MouseButton, MouseButtons, Occlusion,
    PenPhase, PenSample, PenSampler, PenSampling, PenTapAction, PenTool, PointerKind, WindowEvent,
};

use crate::render::trim_renderers;
pub use crate::render::{Canvas, Color, DrawingContext, Point, Rect, RendererBackend};
pub use aurea_render::text::fonts;
use aurea_render::text::trim_caches;

pub use crate::files::{
    FileCache, Stamp, app_cache_dir, app_config_dir, app_data_dir, app_local_data_dir, cache_dir,
    config_dir, data_dir, documents_dir, local_data_dir, open_path, open_url, reveal_path,
};
pub use crate::keymap::{Chord, Keymap};
pub use crate::locale::{locale, locales};
pub use crate::pen::{PenMode, PenSource, pen_mode, pen_source, set_pen_mode};
pub use crate::platform::handles::NativeWindowHandle;
pub use crate::screen::{Screen, screen};
pub use aurea_foundation::{AureaError, AureaResult};
pub use aurea_foundation::{
    Capability, CapabilityChecker, DesktopPlatform, MobilePlatform, OsVersion, Platform, Support,
};

/// Asks for frames at the screen's top rate while `high`, such as while a pen stroke runs, and the
/// system's own pacing after, which saves power. Desktop windows already follow their screen, so
/// there this does nothing.
pub fn set_high_frame_rate(high: bool) {
    #[cfg(target_os = "ios")]
    ios::set_high_frame_rate(high);
    #[cfg(target_os = "android")]
    android::set_high_frame_rate(high);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = high;
}

/// Whether the last pointer press came from a stylus. A check every host answers, so "pen draws,
/// finger pans" reads the same on iOS and Android. False on the desktop, where a pen is a
/// [`WindowEvent::Pen`] and the mouse stays the mouse.
pub fn pointer_is_pencil() -> bool {
    #[cfg(target_os = "ios")]
    return ios::pointer_is_pencil();
    #[cfg(target_os = "android")]
    return android::pointer_is_pencil();
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    false
}

pub use keyboard::{keyboard_connected, keyboard_height, set_keyboard_follows_focus};
pub use sheets::sheet_presented;

static PRESS_ON_HOLD: AtomicBool = AtomicBool::new(true);

/// Whether a finger held still presses after a moment, on iOS and Android: on by default, so a
/// button shows it is held. Off, a still finger waits for a lift, a move or the long press, so a
/// long press leaves no press to cancel, as a paint app wants.
pub fn set_press_on_hold(on: bool) {
    PRESS_ON_HOLD.store(on, Ordering::Relaxed);
}

/// What [`set_press_on_hold`] last set.
pub fn press_on_hold() -> bool {
    PRESS_ON_HOLD.load(Ordering::Relaxed)
}

/// Whether this is a touch host, iOS or Android, where fingers are the pointer.
pub const fn is_touch() -> bool {
    cfg!(any(target_os = "ios", target_os = "android"))
}

/// Plays the system's alert sound, as a terminal's bell does.
pub fn beep() {
    unsafe { ng_platform_beep() }
}

/// Frees the caches that can be rebuilt, for when the system reports memory running low. Call it
/// on the UI thread, where canvases draw; the renderers of canvases on other threads are not
/// reached.
pub fn trim_memory() {
    trim_caches();
    trim_renderers();
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
