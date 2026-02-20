//! Platform capability detection
//!
//! This module provides capability checking to determine which features
//! are available on different platforms. Capabilities differ significantly
//! between desktop and mobile platforms.

use crate::platform::{DesktopPlatform, MobilePlatform, Platform};

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

    pub fn has(&self, capability: Capability) -> bool {
        capability.is_available_on(self.platform)
    }

    pub fn available_capabilities(&self) -> Vec<Capability> {
        use Capability::*;
        [
            MultipleWindows, WindowResizing, WindowMinimization, WindowMaximization,
            FullscreenMode, MenuBar, ContextMenus, KeyboardShortcuts,
            FileDialogs, ColorPicker, FontPicker, SystemNotifications,
            MouseInput, TouchInput, KeyboardInput, StylusInput,
            HardwareAcceleration, OpenGL, Metal, Vulkan, DirectX,
            SystemTray, DockIntegration, TaskbarIntegration, AppIndicators,
            DragAndDrop, Clipboard, ScreenCapture, WindowTransparency, WindowShadows,
        ]
        .iter()
        .copied()
        .filter(|&cap| self.has(cap))
        .collect()
    }

    pub fn unavailable_capabilities(&self) -> Vec<Capability> {
        use Capability::*;
        [
            MultipleWindows, WindowResizing, WindowMinimization, WindowMaximization,
            FullscreenMode, MenuBar, ContextMenus, KeyboardShortcuts,
            FileDialogs, ColorPicker, FontPicker, SystemNotifications,
            MouseInput, TouchInput, KeyboardInput, StylusInput,
            HardwareAcceleration, OpenGL, Metal, Vulkan, DirectX,
            SystemTray, DockIntegration, TaskbarIntegration, AppIndicators,
            DragAndDrop, Clipboard, ScreenCapture, WindowTransparency, WindowShadows,
        ]
        .iter()
        .copied()
        .filter(|&cap| !self.has(cap))
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
    fn test_capability_checker() {
        let checker = CapabilityChecker::new();
        let platform = checker.platform();

        if platform.is_desktop() {
            assert!(checker.has(Capability::MenuBar));
            assert!(checker.has(Capability::MouseInput));
        }

        if platform.is_mobile() {
            assert!(checker.has(Capability::TouchInput));
            assert!(!checker.has(Capability::MenuBar));
        }
    }

    #[test]
    fn test_platform_specific_capabilities() {
        let macos = Platform::Desktop(DesktopPlatform::MacOS);
        let checker = CapabilityChecker::for_platform(macos);
        assert!(checker.has(Capability::Metal));
        assert!(checker.has(Capability::DockIntegration));

        let windows = Platform::Desktop(DesktopPlatform::Windows);
        let checker = CapabilityChecker::for_platform(windows);
        assert!(checker.has(Capability::DirectX));
        assert!(checker.has(Capability::TaskbarIntegration));

        let ios = Platform::Mobile(MobilePlatform::IOS);
        let checker = CapabilityChecker::for_platform(ios);
        assert!(checker.has(Capability::Metal));
        assert!(!checker.has(Capability::MultipleWindows));
    }
}
