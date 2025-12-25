//! Platform capability detection
//!
//! This module provides capability checking to determine which features
//! are available on different platforms. Capabilities differ significantly
//! between desktop and mobile platforms.

use crate::platform::{Platform, DesktopPlatform, MobilePlatform};

/// Represents a capability or feature that may or may not be available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    // Window Management
    /// Multiple windows support
    MultipleWindows,
    /// Window resizing
    WindowResizing,
    /// Window minimization
    WindowMinimization,
    /// Window maximization
    WindowMaximization,
    /// Fullscreen mode
    FullscreenMode,
    
    // Menu System
    /// Menu bar support (desktop only)
    MenuBar,
    /// Context menus
    ContextMenus,
    /// Keyboard shortcuts
    KeyboardShortcuts,
    
    // UI Components
    /// File dialogs (open/save)
    FileDialogs,
    /// Color picker dialogs
    ColorPicker,
    /// Font selection dialogs
    FontPicker,
    /// System notifications
    SystemNotifications,
    
    // Input
    /// Mouse input
    MouseInput,
    /// Touch input
    TouchInput,
    /// Keyboard input
    KeyboardInput,
    /// Stylus/pen input
    StylusInput,
    
    // Rendering
    /// Hardware-accelerated rendering
    HardwareAcceleration,
    /// OpenGL support
    OpenGL,
    /// Metal support (Apple platforms)
    Metal,
    /// Vulkan support
    Vulkan,
    /// DirectX support (Windows)
    DirectX,
    
    // System Integration
    /// System tray/status bar
    SystemTray,
    /// Dock integration (macOS)
    DockIntegration,
    /// Taskbar integration (Windows)
    TaskbarIntegration,
    /// App indicators (Linux)
    AppIndicators,
    
    // Advanced Features
    /// Drag and drop
    DragAndDrop,
    /// Clipboard access
    Clipboard,
    /// Screen capture
    ScreenCapture,
    /// Window transparency
    WindowTransparency,
    /// Window shadows
    WindowShadows,
}

impl Capability {
    /// Check if this capability is available on the given platform
    pub fn is_available_on(&self, platform: Platform) -> bool {
        match platform {
            Platform::Desktop(desktop) => self.is_available_on_desktop(desktop),
            Platform::Mobile(mobile) => self.is_available_on_mobile(mobile),
        }
    }
    
    /// Check if this capability is available on desktop platforms
    fn is_available_on_desktop(&self, desktop: DesktopPlatform) -> bool {
        match self {
            // Window Management - Available on all desktop platforms
            Capability::MultipleWindows => true,
            Capability::WindowResizing => true,
            Capability::WindowMinimization => true,
            Capability::WindowMaximization => true,
            Capability::FullscreenMode => true,
            
            // Menu System - Desktop only
            Capability::MenuBar => true,
            Capability::ContextMenus => true,
            Capability::KeyboardShortcuts => true,
            
            // UI Components
            Capability::FileDialogs => true,
            Capability::ColorPicker => true,
            Capability::FontPicker => true,
            Capability::SystemNotifications => true,
            
            // Input
            Capability::MouseInput => true,
            Capability::TouchInput => false, // Desktop typically doesn't have touch
            Capability::KeyboardInput => true,
            Capability::StylusInput => matches!(desktop, DesktopPlatform::Windows | DesktopPlatform::MacOS), // Some desktops support stylus
            
            // Rendering
            Capability::HardwareAcceleration => true,
            Capability::OpenGL => true,
            Capability::Metal => matches!(desktop, DesktopPlatform::MacOS),
            Capability::Vulkan => matches!(desktop, DesktopPlatform::Linux | DesktopPlatform::Windows),
            Capability::DirectX => matches!(desktop, DesktopPlatform::Windows),
            
            // System Integration
            Capability::SystemTray => true,
            Capability::DockIntegration => matches!(desktop, DesktopPlatform::MacOS),
            Capability::TaskbarIntegration => matches!(desktop, DesktopPlatform::Windows),
            Capability::AppIndicators => matches!(desktop, DesktopPlatform::Linux),
            
            // Advanced Features
            Capability::DragAndDrop => true,
            Capability::Clipboard => true,
            Capability::ScreenCapture => true,
            Capability::WindowTransparency => true,
            Capability::WindowShadows => true,
        }
    }
    
    /// Check if this capability is available on mobile platforms
    fn is_available_on_mobile(&self, mobile: MobilePlatform) -> bool {
        match self {
            // Window Management - Limited on mobile
            Capability::MultipleWindows => false, // Mobile typically single window
            Capability::WindowResizing => false,
            Capability::WindowMinimization => false,
            Capability::WindowMaximization => false,
            Capability::FullscreenMode => true, // Mobile is typically fullscreen
            
            // Menu System - Different on mobile
            Capability::MenuBar => false, // Mobile doesn't have menu bars
            Capability::ContextMenus => true, // Long press menus
            Capability::KeyboardShortcuts => false, // Limited keyboard support
            
            // UI Components
            Capability::FileDialogs => matches!(mobile, MobilePlatform::IOS), // Limited file access
            Capability::ColorPicker => true,
            Capability::FontPicker => false, // Limited font selection
            Capability::SystemNotifications => true,
            
            // Input
            Capability::MouseInput => false, // Mobile doesn't have mouse
            Capability::TouchInput => true, // Primary input method
            Capability::KeyboardInput => true, // Virtual keyboards
            Capability::StylusInput => true, // Supported on many devices
            
            // Rendering
            Capability::HardwareAcceleration => true,
            Capability::OpenGL => matches!(mobile, MobilePlatform::Android), // Android supports OpenGL ES
            Capability::Metal => matches!(mobile, MobilePlatform::IOS), // iOS uses Metal
            Capability::Vulkan => matches!(mobile, MobilePlatform::Android), // Android supports Vulkan
            Capability::DirectX => false, // Windows Mobile is deprecated
            
            // System Integration
            Capability::SystemTray => false, // Mobile doesn't have system tray
            Capability::DockIntegration => false,
            Capability::TaskbarIntegration => false,
            Capability::AppIndicators => false,
            
            // Advanced Features
            Capability::DragAndDrop => matches!(mobile, MobilePlatform::IOS), // iOS 11+ supports drag and drop
            Capability::Clipboard => true,
            Capability::ScreenCapture => true,
            Capability::WindowTransparency => true,
            Capability::WindowShadows => true,
        }
    }
    
    /// Get a human-readable description of the capability
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

/// Capability checker for the current platform
#[derive(Debug, Clone, Copy)]
pub struct CapabilityChecker {
    platform: Platform,
}

impl CapabilityChecker {
    /// Create a new capability checker for the current platform
    pub fn new() -> Self {
        Self {
            platform: Platform::current(),
        }
    }
    
    /// Create a capability checker for a specific platform
    pub fn for_platform(platform: Platform) -> Self {
        Self { platform }
    }
    
    /// Check if a capability is available
    pub fn has(&self, capability: Capability) -> bool {
        capability.is_available_on(self.platform)
    }
    
    /// Get all available capabilities for the current platform
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
    
    /// Get all unavailable capabilities for the current platform
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
    
    /// Get the platform this checker is for
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
        
        // Desktop platforms should have menu bars
        if platform.is_desktop() {
            assert!(checker.has(Capability::MenuBar));
            assert!(checker.has(Capability::MouseInput));
        }
        
        // Mobile platforms should have touch input
        if platform.is_mobile() {
            assert!(checker.has(Capability::TouchInput));
            assert!(!checker.has(Capability::MenuBar));
        }
    }
    
    #[test]
    fn test_platform_specific_capabilities() {
        // macOS should have Metal
        let macos = Platform::Desktop(DesktopPlatform::MacOS);
        let checker = CapabilityChecker::for_platform(macos);
        assert!(checker.has(Capability::Metal));
        assert!(checker.has(Capability::DockIntegration));
        
        // Windows should have DirectX
        let windows = Platform::Desktop(DesktopPlatform::Windows);
        let checker = CapabilityChecker::for_platform(windows);
        assert!(checker.has(Capability::DirectX));
        assert!(checker.has(Capability::TaskbarIntegration));
        
        // iOS should have Metal
        let ios = Platform::Mobile(MobilePlatform::IOS);
        let checker = CapabilityChecker::for_platform(ios);
        assert!(checker.has(Capability::Metal));
        assert!(!checker.has(Capability::MultipleWindows));
    }
}

