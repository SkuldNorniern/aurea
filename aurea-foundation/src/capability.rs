//! What can be done here.
//!
//! There are two different questions and they had been answered as one.
//!
//! [`Capability::is_available_on`] says what the *platform* can do. Windows has
//! file dialogs, macOS has Metal. That is background knowledge, and it is true
//! whether or not Aurea does anything with it.
//!
//! [`Capability::support_on`] says what *Aurea* can do here, which is the
//! question an application is really asking when it checks. Aurea implements a
//! small part of what the platforms offer, so most of these come back
//! [`Support::Unimplemented`] even where the platform underneath is perfectly
//! capable. Answering with the platform's abilities would tell an application
//! it can open a file dialog that no code exists to open.

use crate::platform::{DesktopPlatform, MobilePlatform, Platform};

/// How well Aurea supports something on a platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Support {
    /// The platform cannot do it at all.
    Unavailable,
    /// The platform can, and Aurea has no code for it yet.
    Unimplemented,
    /// Aurea does it, but the implementation is young enough to be worth
    /// saying so.
    Experimental,
    /// Aurea does it.
    Supported,
}

impl Support {
    /// Whether an application can use it. `Experimental` counts.
    pub fn is_usable(self) -> bool {
        matches!(self, Self::Supported | Self::Experimental)
    }
}

/// Represents a capability or feature that may or may not be available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    MultipleWindows,
    WindowResizing,
    WindowMinimization,
    WindowMaximization,
    FullscreenMode,
    MenuBar,
    ContextMenus,
    KeyboardShortcuts,
    FileDialogs,
    ColorPicker,
    FontPicker,
    SystemNotifications,
    MouseInput,
    TouchInput,
    KeyboardInput,
    StylusInput,
    HardwareAcceleration,
    OpenGL,
    Metal,
    Vulkan,
    DirectX,
    SystemTray,
    DockIntegration,
    TaskbarIntegration,
    AppIndicators,
    DragAndDrop,
    Clipboard,
    ScreenCapture,
    WindowTransparency,
    WindowShadows,
}

impl Capability {
    pub fn is_available_on(&self, platform: Platform) -> bool {
        match platform {
            Platform::Desktop(desktop) => self.is_available_on_desktop(desktop),
            Platform::Mobile(mobile) => self.is_available_on_mobile(mobile),
        }
    }

    fn is_available_on_desktop(&self, desktop: DesktopPlatform) -> bool {
        match self {
            Capability::MultipleWindows => true,
            Capability::WindowResizing => true,
            Capability::WindowMinimization => true,
            Capability::WindowMaximization => true,
            Capability::FullscreenMode => true,
            Capability::MenuBar => true,
            Capability::ContextMenus => true,
            Capability::KeyboardShortcuts => true,
            Capability::FileDialogs => true,
            Capability::ColorPicker => true,
            Capability::FontPicker => true,
            Capability::SystemNotifications => true,
            Capability::MouseInput => true,
            Capability::TouchInput => false,
            Capability::KeyboardInput => true,
            Capability::StylusInput => {
                matches!(desktop, DesktopPlatform::Windows | DesktopPlatform::MacOS)
            }
            Capability::HardwareAcceleration => true,
            Capability::OpenGL => true,
            Capability::Metal => matches!(desktop, DesktopPlatform::MacOS),
            Capability::Vulkan => {
                matches!(desktop, DesktopPlatform::Linux | DesktopPlatform::Windows)
            }
            Capability::DirectX => matches!(desktop, DesktopPlatform::Windows),
            Capability::SystemTray => true,
            Capability::DockIntegration => matches!(desktop, DesktopPlatform::MacOS),
            Capability::TaskbarIntegration => matches!(desktop, DesktopPlatform::Windows),
            Capability::AppIndicators => matches!(desktop, DesktopPlatform::Linux),
            Capability::DragAndDrop => true,
            Capability::Clipboard => true,
            Capability::ScreenCapture => true,
            Capability::WindowTransparency => true,
            Capability::WindowShadows => true,
        }
    }

    fn is_available_on_mobile(&self, mobile: MobilePlatform) -> bool {
        match self {
            Capability::MultipleWindows => false,
            Capability::WindowResizing => false,
            Capability::WindowMinimization => false,
            Capability::WindowMaximization => false,
            Capability::FullscreenMode => true,
            Capability::MenuBar => false,
            Capability::ContextMenus => true,
            Capability::KeyboardShortcuts => false,
            Capability::FileDialogs => matches!(mobile, MobilePlatform::IOS),
            Capability::ColorPicker => true,
            Capability::FontPicker => false,
            Capability::SystemNotifications => true,
            Capability::MouseInput => false,
            Capability::TouchInput => true,
            Capability::KeyboardInput => true,
            Capability::StylusInput => true,
            Capability::HardwareAcceleration => true,
            Capability::OpenGL => matches!(mobile, MobilePlatform::Android),
            Capability::Metal => matches!(mobile, MobilePlatform::IOS),
            Capability::Vulkan => matches!(mobile, MobilePlatform::Android),
            Capability::DirectX => false,
            Capability::SystemTray => false,
            Capability::DockIntegration => false,
            Capability::TaskbarIntegration => false,
            Capability::AppIndicators => false,
            Capability::DragAndDrop => matches!(mobile, MobilePlatform::IOS),
            Capability::Clipboard => true,
            Capability::ScreenCapture => true,
            Capability::WindowTransparency => true,
            Capability::WindowShadows => true,
        }
    }

    /// What Aurea can do with this capability on `platform`.
    ///
    /// Built from what the FFI layer actually declares, not from what the
    /// platform is able to do.
    pub fn support_on(&self, platform: Platform) -> Support {
        if !self.is_available_on(platform) {
            return Support::Unavailable;
        }
        let desktop = platform.is_desktop();
        match self {
            // Windows, sizing, position and visibility are all in the FFI.
            Self::MultipleWindows | Self::WindowResizing => Support::Supported,
            // Menus exist on desktop through create_menu / add_menu_item.
            Self::MenuBar | Self::KeyboardShortcuts => Support::Supported,
            // Events are delivered for these.
            Self::MouseInput | Self::KeyboardInput => Support::Supported,
            Self::Clipboard => Support::Supported,
            // A canvas can hand out a GPU surface for any of these, but
            // Aurea's own renderer only draws through ZenGPU on Vulkan;
            // anything else is the application's renderer on Aurea's surface.
            Self::HardwareAcceleration
            | Self::Vulkan
            | Self::Metal
            | Self::DirectX
            | Self::OpenGL => Support::Experimental,
            // Touch and stylus arrive as mouse events at best; there is no
            // separate event for them yet.
            Self::TouchInput | Self::StylusInput => Support::Unimplemented,
            // Minimise and maximise are reported as lifecycle events but
            // cannot be asked for.
            Self::WindowMinimization | Self::WindowMaximization if desktop => {
                Support::Unimplemented
            }
            // Nothing in the FFI covers the rest: no file dialog, no picker,
            // no tray, no notifications, no transparency, no capture.
            _ => Support::Unimplemented,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Capability::MultipleWindows => "Multiple Windows",
            Capability::WindowResizing => "Window Resizing",
            Capability::WindowMinimization => "Window Minimization",
            Capability::WindowMaximization => "Window Maximization",
            Capability::FullscreenMode => "Fullscreen Mode",
            Capability::MenuBar => "Menu Bar",
            Capability::ContextMenus => "Context Menus",
            Capability::KeyboardShortcuts => "Keyboard Shortcuts",
            Capability::FileDialogs => "File Dialogs",
            Capability::ColorPicker => "Color Picker",
            Capability::FontPicker => "Font Picker",
            Capability::SystemNotifications => "System Notifications",
            Capability::MouseInput => "Mouse Input",
            Capability::TouchInput => "Touch Input",
            Capability::KeyboardInput => "Keyboard Input",
            Capability::StylusInput => "Stylus/Pen Input",
            Capability::HardwareAcceleration => "Hardware Acceleration",
            Capability::OpenGL => "OpenGL",
            Capability::Metal => "Metal",
            Capability::Vulkan => "Vulkan",
            Capability::DirectX => "DirectX",
            Capability::SystemTray => "System Tray",
            Capability::DockIntegration => "Dock Integration",
            Capability::TaskbarIntegration => "Taskbar Integration",
            Capability::AppIndicators => "App Indicators",
            Capability::DragAndDrop => "Drag and Drop",
            Capability::Clipboard => "Clipboard",
            Capability::ScreenCapture => "Screen Capture",
            Capability::WindowTransparency => "Window Transparency",
            Capability::WindowShadows => "Window Shadows",
        }
    }
}

const ALL: &[Capability] = &[
    Capability::MultipleWindows,
    Capability::WindowResizing,
    Capability::WindowMinimization,
    Capability::WindowMaximization,
    Capability::FullscreenMode,
    Capability::MenuBar,
    Capability::ContextMenus,
    Capability::KeyboardShortcuts,
    Capability::FileDialogs,
    Capability::ColorPicker,
    Capability::FontPicker,
    Capability::SystemNotifications,
    Capability::MouseInput,
    Capability::TouchInput,
    Capability::KeyboardInput,
    Capability::StylusInput,
    Capability::HardwareAcceleration,
    Capability::OpenGL,
    Capability::Metal,
    Capability::Vulkan,
    Capability::DirectX,
    Capability::SystemTray,
    Capability::DockIntegration,
    Capability::TaskbarIntegration,
    Capability::AppIndicators,
    Capability::DragAndDrop,
    Capability::Clipboard,
    Capability::ScreenCapture,
    Capability::WindowTransparency,
    Capability::WindowShadows,
];

#[derive(Debug, Clone, Copy)]
pub struct CapabilityChecker {
    platform: Platform,
}

impl CapabilityChecker {
    pub fn new() -> Self {
        Self {
            platform: Platform::current(),
        }
    }

    pub fn for_platform(platform: Platform) -> Self {
        Self { platform }
    }

    /// Whether Aurea can do this here.
    ///
    /// This asks about Aurea, not about the platform. Use
    /// [`Capability::is_available_on`] for the platform's own abilities.
    pub fn has(&self, capability: Capability) -> bool {
        self.support(capability).is_usable()
    }

    /// How well Aurea supports this here.
    pub fn support(&self, capability: Capability) -> Support {
        capability.support_on(self.platform)
    }

    /// Everything Aurea can do here.
    pub fn available_capabilities(&self) -> Vec<Capability> {
        ALL.iter().copied().filter(|&cap| self.has(cap)).collect()
    }

    /// Everything it cannot, whether because the platform lacks it or because
    /// Aurea has no code for it.
    pub fn unavailable_capabilities(&self) -> Vec<Capability> {
        ALL.iter().copied().filter(|&cap| !self.has(cap)).collect()
    }

    /// The capabilities the platform offers that Aurea has not implemented.
    pub fn unimplemented_capabilities(&self) -> Vec<Capability> {
        ALL.iter()
            .copied()
            .filter(|&cap| self.support(cap) == Support::Unimplemented)
            .collect()
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }
}

impl Default for CapabilityChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_checker_reports_what_aurea_does_not_what_the_platform_could() {
        let checker = CapabilityChecker::for_platform(Platform::Desktop(DesktopPlatform::Windows));

        // Windows has file dialogs. Aurea has no code that opens one, so an
        // application must not be told it can.
        assert!(Capability::FileDialogs.is_available_on(checker.platform()));
        assert_eq!(
            checker.support(Capability::FileDialogs),
            Support::Unimplemented
        );
        assert!(!checker.has(Capability::FileDialogs));
    }

    #[test]
    fn the_things_aurea_does_are_reported_as_supported() {
        let checker = CapabilityChecker::for_platform(Platform::Desktop(DesktopPlatform::Linux));

        assert_eq!(checker.support(Capability::MenuBar), Support::Supported);
        assert_eq!(checker.support(Capability::Clipboard), Support::Supported);
        assert_eq!(
            checker.support(Capability::KeyboardInput),
            Support::Supported
        );
        assert!(checker.has(Capability::MenuBar));
    }

    #[test]
    fn what_the_platform_cannot_do_is_unavailable_not_unimplemented() {
        let checker = CapabilityChecker::for_platform(Platform::Mobile(MobilePlatform::Android));

        assert_eq!(checker.support(Capability::MenuBar), Support::Unavailable);
        assert!(!checker.has(Capability::MenuBar));
    }

    #[test]
    fn gpu_surfaces_are_experimental_but_usable() {
        let checker = CapabilityChecker::for_platform(Platform::Desktop(DesktopPlatform::MacOS));

        assert_eq!(checker.support(Capability::Metal), Support::Experimental);
        assert!(checker.has(Capability::Metal), "experimental still counts");
    }

    #[test]
    fn touch_is_not_claimed_before_there_are_touch_events() {
        let checker = CapabilityChecker::for_platform(Platform::Mobile(MobilePlatform::IOS));

        // The platform has touch; Aurea has no touch event yet.
        assert!(Capability::TouchInput.is_available_on(checker.platform()));
        assert_eq!(
            checker.support(Capability::TouchInput),
            Support::Unimplemented
        );
    }

    #[test]
    fn the_three_lists_partition_everything() {
        let checker = CapabilityChecker::for_platform(Platform::Desktop(DesktopPlatform::Windows));
        let available = checker.available_capabilities().len();
        let unavailable = checker.unavailable_capabilities().len();

        assert_eq!(available + unavailable, ALL.len());
        assert!(
            !checker.unimplemented_capabilities().is_empty(),
            "plenty is unimplemented and saying so is the point"
        );
    }

    #[test]
    fn experimental_is_usable_and_the_rest_are_not() {
        assert!(Support::Supported.is_usable());
        assert!(Support::Experimental.is_usable());
        assert!(!Support::Unimplemented.is_usable());
        assert!(!Support::Unavailable.is_usable());
    }

    #[test]
    fn every_capability_has_a_description() {
        for cap in ALL {
            assert!(!cap.description().is_empty(), "{cap:?} has no description");
        }
    }
}
