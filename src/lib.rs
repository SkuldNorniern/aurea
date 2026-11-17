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
//!
//! # Where things are
//!
//! - [`Window`]: a window, its events through [`Window::on_event`], and the run loop.
//! - [`elements`]: native widgets and layout, the system's own controls.
//! - [`render`]: [`Canvas`] and what draws on it, [`DrawingContext`] and [`render::Paint`].
//! - [`ui`]: widgets drawn on a canvas, the same on every platform, styled by a [`ui::Theme`].
//! - [`animation`]: eased values, for use in a frame.
//! - [`menu`], [`tray`], [`dialog`], [`clipboard`], [`files`]: the rest of the desktop.
//! - [`prelude`]: the types most apps need, to glob-import. It holds the native widgets, so
//!   [`ui`]'s widgets of the same name are imported by path.
//!
//! # A canvas draws when asked
//!
//! [`Canvas::set_draw_callback`] records a frame, and damage tracking repaints what changed.
//! Ask for a frame with [`render::request_canvas_redraw`], or run [`Canvas::on_frame`] to animate:
//! it runs every frame until it returns `false`, and [`FrameInfo::delta`] is the time to move by.
//!
//! # Threads
//!
//! Windows, widgets and canvases stay on the UI thread, the one that made the first window; on
//! macOS and iOS that has to be the main thread. From other threads, [`on_ui_thread`] queues a
//! closure and [`Window::proxy`] gives a [`WindowProxy`].
//! In a draw callback, [`after_frame`] holds work that needs nothing borrowed, such as opening a
//! file dialog.
//!
//! # Features
//!
//! Every feature below is on by default, so an app changes nothing; `default-features = false`
//! leaves out what the app does not use. The CPU renderer always works, and keys and the mouse are
//! always on.
//!
//! - `input`: keyboard, mouse, text, pen, touch, gestures and file drops. Its parts are
//!   `text-input`, `ime`, `raw-mouse`, `pen`, `touch`, `gestures`, `drop-files` and `gamepad`, and
//!   [`input::supported`] says which are there.
//! - `gamepad`: game controllers, polled through [`input::gamepad::Pads`]. Windows and Linux.
//! - `clipboard`: clipboard text and images. Text fields copy and paste only with it.
//! - `files`: opening, revealing and sharing files, and the app's folders. Pick and save on the
//!   desktop need `dialog` too; on phones they are part of `files`.
//! - `menu`: menu bars, [`Window::create_menu_bar`]. The menu tree also draws an in-app popup, so
//!   [`menu::MenuSpec`] needs `ui-core`.
//! - `tray`: icons in the notification area, [`TrayIcon`].
//! - `dialog`: native file, message and colour dialogs.
//! - `single-instance`: [`app::single_instance`], which needs `files`.
//! - `embed`: SwiftUI hosts on macOS and the C embedding API.
//! - `widgets`: the native controls in [`elements`]. Layout and canvases stay without it.
//! - `ui`: the canvas widget toolkit, [`app::run`] and [`animation`]. Its parts are `ui-core`,
//!   `ui-graph`, `ui-dock`, `ui-color` and `ui-text-edit`; `ui` is all of them.
//! - `ui-core`: the toolkit without the parts below.
//! - `ui-graph`: plots and scope views, [`ui::graph`].
//! - `ui-dock`: docking panels, [`ui::Dock`].
//! - `ui-color`: colour picker, colour wheel and swatches.
//! - `ui-text-edit`: multiline [`ui::TextArea`].
//! - `effects`: backdrop blur, frost and other filters on the CPU renderer; off, a backdrop draws
//!   nothing.
//! - `system-fonts`: installed fonts by name and for fallback; off, only fonts the app registers,
//!   then the built-in face.
//! - `zengpu`: GPU rendering through ZenGPU, on Metal on macOS and Vulkan elsewhere. Experimental.
//! - `wgpu`: wgpu renderer and surface helpers, for fitting Aurea into a wgpu app.
//! - `accessibility`: screen reader support for canvas widgets, through AccessKit.
//! - `mobile`: iOS and Android hosts. They draw with `ui`, so it brings `ui`, `dialog`, `files`,
//!   `menu` and `clipboard` with it.
//!
//! # App identity
//!
//! An app's name, id and icon live in its `Cargo.toml`. The `aurea-build` crate reads them in the
//! app's `build.rs`, and [`identity!`] picks them up.

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
mod wake;

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

#[cfg(feature = "ui-core")]
pub use aurea_animation as animation;

pub mod app;
mod device;
pub mod dialog;
mod dispatch;
pub mod elements;
#[cfg(feature = "embed")]
pub mod embed;
/// C ABI entry points for the native layers.
pub mod ffi;
#[cfg(feature = "files")]
pub mod files;
mod image;
pub mod input;
pub mod integration;
#[cfg(feature = "ui-core")]
pub(crate) use input::keyboard;
pub use input::keymap;
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
mod screen;
#[cfg(feature = "files")]
mod sheets;
#[cfg(feature = "tray")]
pub mod tray;
#[cfg(feature = "ui-core")]
pub mod ui;
pub mod window;

pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};

pub use crate::dispatch::{
    LateWakeHandle, WakeHandle, after_frame, on_ui_thread, run_after, wake_canvas, wake_handle,
};
#[cfg(feature = "widgets")]
pub use crate::elements::{
    Button, Checkbox, ComboBox, ImageView, Label, ProgressBar, RadioGroup, RowList, ScrollView,
    ScrollbarPolicy, SidebarList, Slider, Spacer, SpinBox, SplitOrientation, SplitView, TabBar,
    Table, TextEditor, TextField, TextView,
};
pub use crate::elements::{Container, Divider, Element, Orientation, Stack};
use crate::ffi::{
    ng_platform_beep, ng_platform_prefers_dark, ng_platform_reduced_motion, ng_platform_wheel_lines,
};
pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};
#[cfg(feature = "tray")]
pub use crate::tray::TrayIcon;
pub use crate::window::clipboard;
pub use crate::window::{
    CloseAnswer, CursorGrabMode, CursorIcon, Window, WindowId, WindowManager, WindowPlacement,
    WindowProxy, WindowType,
};
pub use crate::window::{clipboard_text, set_clipboard_text};

pub use crate::window::{
    EventCallback, GesturePhase, KeyCode, Modifiers, MouseButton, MouseButtons, Occlusion,
    PenPhase, PenSample, PenSampler, PenSampling, PenTapAction, PenTool, PointerKind, ScrollEvent,
    ScrollPhase, ScrollSource, WindowEvent,
};

use crate::render::trim_renderers;
pub use crate::render::{Canvas, Color, DrawingContext, Point, Rect, RendererBackend};
pub use aurea_render::text::fonts;
use aurea_render::text::trim_caches;

#[cfg(feature = "files")]
pub use crate::files::{
    FileCache, Stamp, app_cache_dir, app_config_dir, app_data_dir, app_local_data_dir, cache_dir,
    config_dir, data_dir, documents_dir, local_data_dir, open_path, open_url, reveal_path,
};
pub use crate::input::keymap::{Chord, Keymap};
pub use crate::input::pen::{PenMode, PenSource, pen_mode, pen_source, set_pen_mode};
pub use crate::locale::{locale, locales};
pub use crate::platform::handles::NativeWindowHandle;
pub use crate::screen::{Screen, screen, simulate_screen};
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

pub use crate::input::keyboard::{keyboard_connected, keyboard_height, set_keyboard_follows_focus};
pub use device::{Device, Thermal, device};
#[cfg(feature = "files")]
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

/// Logical pixels a wheel click scrolls under the user's line setting: 16 a line, 48 when it is
/// a page.
pub(crate) fn wheel_line_pixels() -> f32 {
    match wheel_scroll() {
        WheelScroll::Lines(lines) => 16.0 * f32::from(u16::try_from(lines).unwrap_or(u16::MAX)),
        WheelScroll::Page => 48.0,
    }
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
