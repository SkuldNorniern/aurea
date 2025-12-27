//! Platform detection and platform-specific functionality
//!
//! This module provides platform detection and categorization, distinguishing
//! between desktop and mobile platforms.
//!
//! **Mobile vs Desktop Differences:**
//! - **Mobile platforms** (iOS, Android):
//!   - Single window model (fullscreen by default)
//!   - Lifecycle-driven (background/foreground, pause/resume)
//!   - Touch input primary, no mouse
//!   - Scale factor changes with device rotation
//!   - Memory constraints more strict
//!   - Surface recreation on context loss
//!
//! - **Desktop platforms** (macOS, Windows, Linux):
//!   - Multiple windows supported
//!   - Mouse/keyboard input primary
//!   - Window management (minimize, maximize, resize)
//!   - Scale factor changes when moving between displays
//!   - More relaxed memory constraints

/// Represents the target platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Desktop platforms
    Desktop(DesktopPlatform),
    /// Mobile platforms
    Mobile(MobilePlatform),
}

/// Desktop operating systems
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DesktopPlatform {
    /// macOS (Apple desktop)
    MacOS,
    /// Windows (Microsoft)
    Windows,
    /// Linux (various distributions)
    Linux,
}

/// Mobile operating systems
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MobilePlatform {
    /// iOS (Apple mobile)
    IOS,
    /// Android (Google mobile)
    Android,
}

impl Platform {
    /// Detect the current platform at compile time
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Platform::Desktop(DesktopPlatform::MacOS)
        }
        
        #[cfg(target_os = "windows")]
        {
            Platform::Desktop(DesktopPlatform::Windows)
        }
        
        #[cfg(target_os = "linux")]
        {
            Platform::Desktop(DesktopPlatform::Linux)
        }
        
        #[cfg(target_os = "ios")]
        {
            Platform::Mobile(MobilePlatform::IOS)
        }
        
        #[cfg(target_os = "android")]
        {
            Platform::Mobile(MobilePlatform::Android)
        }
        
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "ios",
            target_os = "android"
        )))]
        {
            compile_error!("Unsupported platform")
        }
    }
    
    /// Check if this is a desktop platform
    pub fn is_desktop(&self) -> bool {
        matches!(self, Platform::Desktop(_))
    }
    
    /// Check if this is a mobile platform
    pub fn is_mobile(&self) -> bool {
        matches!(self, Platform::Mobile(_))
    }
    
    /// Get the desktop platform variant, if applicable
    pub fn as_desktop(&self) -> Option<DesktopPlatform> {
        match self {
            Platform::Desktop(platform) => Some(*platform),
            Platform::Mobile(_) => None,
        }
    }
    
    /// Get the mobile platform variant, if applicable
    pub fn as_mobile(&self) -> Option<MobilePlatform> {
        match self {
            Platform::Desktop(_) => None,
            Platform::Mobile(platform) => Some(*platform),
        }
    }
    
    /// Get the platform name as a string
    pub fn name(&self) -> &'static str {
        match self {
            Platform::Desktop(DesktopPlatform::MacOS) => "macOS",
            Platform::Desktop(DesktopPlatform::Windows) => "Windows",
            Platform::Desktop(DesktopPlatform::Linux) => "Linux",
            Platform::Mobile(MobilePlatform::IOS) => "iOS",
            Platform::Mobile(MobilePlatform::Android) => "Android",
        }
    }
    
    /// Get the platform family (desktop or mobile)
    pub fn family(&self) -> &'static str {
        match self {
            Platform::Desktop(_) => "Desktop",
            Platform::Mobile(_) => "Mobile",
        }
    }
}

impl DesktopPlatform {
    /// Get the desktop platform name
    pub fn name(&self) -> &'static str {
        match self {
            DesktopPlatform::MacOS => "macOS",
            DesktopPlatform::Windows => "Windows",
            DesktopPlatform::Linux => "Linux",
        }
    }
}

impl MobilePlatform {
    /// Get the mobile platform name
    pub fn name(&self) -> &'static str {
        match self {
            MobilePlatform::IOS => "iOS",
            MobilePlatform::Android => "Android",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name(), self.family())
    }
}

impl std::fmt::Display for DesktopPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::fmt::Display for MobilePlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_platform_detection() {
        let platform = Platform::current();
        assert!(platform.is_desktop() || platform.is_mobile());
    }
    
    #[test]
    fn test_platform_display() {
        let platform = Platform::current();
        let display = format!("{}", platform);
        assert!(!display.is_empty());
    }
}

