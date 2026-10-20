//! The running operating system's release, for gating features on newer systems.

use std::sync::OnceLock;

/// An OS release: Windows major.minor.build, macOS and iOS major.minor.patch, or the
/// Linux and Android kernel's major.minor.patch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OsVersion {
    /// Major release number.
    pub major: u32,
    /// Minor release number.
    pub minor: u32,
    /// Windows build number, or the patch level elsewhere.
    pub build: u32,
}

impl OsVersion {
    /// Windows XP.
    pub const WINDOWS_XP: Self = Self::new(5, 1, 2600);
    /// Windows Vista.
    pub const WINDOWS_VISTA: Self = Self::new(6, 0, 6000);
    /// Windows 7.
    pub const WINDOWS_7: Self = Self::new(6, 1, 7600);
    /// Windows 8.
    pub const WINDOWS_8: Self = Self::new(6, 2, 9200);
    /// Windows 8.1.
    pub const WINDOWS_8_1: Self = Self::new(6, 3, 9600);
    /// Windows 10, first release.
    pub const WINDOWS_10: Self = Self::new(10, 0, 10240);
    /// Windows 11, first release.
    pub const WINDOWS_11: Self = Self::new(10, 0, 22000);

    /// A version from its parts.
    pub const fn new(major: u32, minor: u32, build: u32) -> Self {
        Self {
            major,
            minor,
            build,
        }
    }

    /// The running system's version; all zeros when it cannot be read.
    pub fn current() -> Self {
        static CURRENT: OnceLock<OsVersion> = OnceLock::new();
        *CURRENT.get_or_init(|| read().unwrap_or_default())
    }

    /// Whether this version is `other` or newer.
    pub fn at_least(self, other: Self) -> bool {
        self >= other
    }

    /// The first three numbers in `text`, such as `"14.5"` or `"6.6.87.2-generic"`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text
            .split(|c: char| !c.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .map(str::parse::<u32>);
        let major = parts.next()?.ok()?;
        let minor = parts.next().and_then(Result::ok).unwrap_or(0);
        let build = parts.next().and_then(Result::ok).unwrap_or(0);
        Some(Self::new(major, minor, build))
    }
}

#[cfg(target_os = "windows")]
fn read() -> Option<OsVersion> {
    #[repr(C)]
    struct VersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        service_pack: [u16; 128],
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(info: *mut VersionInfo) -> i32;
    }

    let mut info = VersionInfo {
        size: u32::try_from(size_of::<VersionInfo>()).ok()?,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        service_pack: [0; 128],
    };
    let status = unsafe { RtlGetVersion(&raw mut info) };
    (status == 0).then(|| OsVersion::new(info.major, info.minor, info.build))
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn read() -> Option<OsVersion> {
    use std::ffi::{CStr, c_char, c_int, c_void};

    unsafe extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            old: *mut c_void,
            old_len: *mut usize,
            new: *mut c_void,
            new_len: usize,
        ) -> c_int;
    }

    let mut buf = [0u8; 64];
    let mut len = buf.len();
    let status = unsafe {
        sysctlbyname(
            c"kern.osproductversion".as_ptr(),
            buf.as_mut_ptr().cast(),
            &raw mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    OsVersion::parse(CStr::from_bytes_until_nul(&buf).ok()?.to_str().ok()?)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn read() -> Option<OsVersion> {
    use std::ffi::{CStr, c_char, c_int};

    #[repr(C)]
    struct UtsName {
        sysname: [c_char; 65],
        nodename: [c_char; 65],
        release: [c_char; 65],
        version: [c_char; 65],
        machine: [c_char; 65],
        domainname: [c_char; 65],
    }

    unsafe extern "C" {
        fn uname(buf: *mut UtsName) -> c_int;
    }

    let mut name = UtsName {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
        domainname: [0; 65],
    };
    if unsafe { uname(&raw mut name) } != 0 {
        return None;
    }
    let release = unsafe { CStr::from_ptr(name.release.as_ptr()) };
    OsVersion::parse(release.to_str().ok()?)
}

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
fn read() -> Option<OsVersion> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_from_release_strings() {
        assert_eq!(OsVersion::parse("14.5"), Some(OsVersion::new(14, 5, 0)));
        assert_eq!(
            OsVersion::parse("6.6.87.2-microsoft-standard-WSL2"),
            Some(OsVersion::new(6, 6, 87))
        );
        assert_eq!(OsVersion::parse("10"), Some(OsVersion::new(10, 0, 0)));
        assert_eq!(OsVersion::parse("beta"), None);
    }

    #[test]
    fn versions_order_by_major_minor_build() {
        assert!(OsVersion::WINDOWS_11.at_least(OsVersion::WINDOWS_10));
        assert!(OsVersion::WINDOWS_7.at_least(OsVersion::WINDOWS_XP));
        assert!(!OsVersion::WINDOWS_8.at_least(OsVersion::WINDOWS_8_1));
    }

    #[test]
    fn the_running_system_reports_a_version() {
        let current = OsVersion::current();
        assert!(current.major > 0, "{current:?}");
        #[cfg(target_os = "windows")]
        assert!(current.at_least(OsVersion::WINDOWS_XP), "{current:?}");
    }
}
