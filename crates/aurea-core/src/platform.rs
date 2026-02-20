//! Platform detection and platform-specific functionality

/// Represents the target platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Desktop(DesktopPlatform),
    Mobile(MobilePlatform),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DesktopPlatform {
    MacOS,
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MobilePlatform {
    IOS,
    Android,
}

impl Platform {
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

    pub fn is_desktop(&self) -> bool {
        matches!(self, Platform::Desktop(_))
    }

    pub fn is_mobile(&self) -> bool {
        matches!(self, Platform::Mobile(_))
    }

    pub fn as_desktop(&self) -> Option<DesktopPlatform> {
        match self {
            Platform::Desktop(platform) => Some(*platform),
            Platform::Mobile(_) => None,
        }
    }

    pub fn as_mobile(&self) -> Option<MobilePlatform> {
        match self {
            Platform::Desktop(_) => None,
            Platform::Mobile(platform) => Some(*platform),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Platform::Desktop(DesktopPlatform::MacOS) => "macOS",
            Platform::Desktop(DesktopPlatform::Windows) => "Windows",
            Platform::Desktop(DesktopPlatform::Linux) => "Linux",
            Platform::Mobile(MobilePlatform::IOS) => "iOS",
            Platform::Mobile(MobilePlatform::Android) => "Android",
        }
    }

    pub fn family(&self) -> &'static str {
        match self {
            Platform::Desktop(_) => "Desktop",
            Platform::Mobile(_) => "Mobile",
        }
    }
}

impl DesktopPlatform {
    pub fn name(&self) -> &'static str {
        match self {
            DesktopPlatform::MacOS => "macOS",
            DesktopPlatform::Windows => "Windows",
            DesktopPlatform::Linux => "Linux",
        }
    }
}

impl MobilePlatform {
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
