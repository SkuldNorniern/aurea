//! What the device can spare: memory, heat and battery saving, so an app can size its caches and
//! back off. On iOS and Android, [`crate::WindowEvent::DeviceChanged`] says when heat or battery
//! saving moved.

#[cfg(target_os = "android")]
use crate::android;
#[cfg(target_os = "linux")]
use std::fs;

/// How hot the device says it is running.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Thermal {
    /// No limits, or the platform doesn't say.
    #[default]
    Nominal,
    /// A little warm: fine to carry on, not the time for extra work.
    Fair,
    /// Getting throttled: ease off.
    Serious,
    /// About to be stopped: do as little as possible.
    Critical,
}

/// What [`device`] finds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Device {
    /// All the memory in the device, in bytes; 0 if unknown.
    pub total_memory: u64,
    /// What this app may use before the system steps in, in bytes, where the platform has such a
    /// limit: a phone or tablet.
    pub app_memory: Option<u64>,
    /// How hot it is running.
    pub thermal: Thermal,
    /// Whether battery saving is on.
    pub low_power: bool,
}

/// The device now. Cheap enough to ask when sizing something, not every frame.
pub fn device() -> Device {
    query()
}

/// Android's thermal status, 0 to 6.
#[cfg_attr(not(any(test, target_os = "android")), allow(dead_code))]
fn from_android_status(status: i64) -> Thermal {
    match status {
        i64::MIN..=0 => Thermal::Nominal,
        1 | 2 => Thermal::Fair,
        3 => Thermal::Serious,
        _ => Thermal::Critical,
    }
}

/// `NSProcessInfoThermalState`, 0 to 3.
#[cfg_attr(
    not(any(test, target_os = "ios", target_os = "macos")),
    allow(dead_code)
)]
fn from_apple_state(state: i32) -> Thermal {
    match state {
        i32::MIN..=0 => Thermal::Nominal,
        1 => Thermal::Fair,
        2 => Thermal::Serious,
        _ => Thermal::Critical,
    }
}

/// `MemTotal` from the text of `/proc/meminfo`, in bytes.
#[cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]
fn meminfo_total(text: &str) -> Option<u64> {
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    kilobytes.checked_mul(1024)
}

#[cfg(windows)]
fn query() -> Device {
    #[repr(C)]
    #[derive(Default)]
    struct MemoryStatus {
        length: u32,
        load: u32,
        total: u64,
        rest: [u64; 6],
    }
    #[repr(C)]
    #[derive(Default)]
    struct PowerStatus {
        ac: u8,
        battery: u8,
        percent: u8,
        flags: u8,
        seconds: [u32; 2],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32;
        fn GetSystemPowerStatus(status: *mut PowerStatus) -> i32;
    }
    let mut memory = MemoryStatus {
        length: u32::try_from(size_of::<MemoryStatus>()).unwrap_or(0),
        ..MemoryStatus::default()
    };
    let mut power = PowerStatus::default();
    // SAFETY: both fill the struct they are given, whose layout is the system's.
    let (known, saving) = unsafe {
        (
            GlobalMemoryStatusEx(&raw mut memory) != 0,
            GetSystemPowerStatus(&raw mut power) != 0 && power.flags & 1 != 0,
        )
    };
    Device {
        total_memory: if known { memory.total } else { 0 },
        low_power: saving,
        ..Device::default()
    }
}

#[cfg(target_os = "linux")]
fn query() -> Device {
    let text = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    Device {
        total_memory: meminfo_total(&text).unwrap_or(0),
        ..Device::default()
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn query() -> Device {
    unsafe extern "C" {
        fn ng_apple_device_info(
            total: *mut u64,
            available: *mut u64,
            thermal: *mut i32,
            low: *mut i32,
        );
    }
    let (mut total, mut available, mut thermal, mut low) = (0, 0, 0, 0);
    // SAFETY: the host only writes through these for the length of the call.
    unsafe {
        ng_apple_device_info(
            &raw mut total,
            &raw mut available,
            &raw mut thermal,
            &raw mut low,
        );
    }
    Device {
        total_memory: total,
        app_memory: (available > 0).then_some(available),
        thermal: from_apple_state(thermal),
        low_power: low != 0,
    }
}

#[cfg(target_os = "android")]
fn query() -> Device {
    let [total, limit, thermal, low] = android::device_values();
    Device {
        total_memory: u64::try_from(total).unwrap_or(0),
        app_memory: u64::try_from(limit).ok().filter(|limit| *limit > 0),
        thermal: from_android_status(thermal),
        low_power: low != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_scales_become_ours() {
        assert_eq!(from_android_status(0), Thermal::Nominal);
        assert_eq!(from_android_status(2), Thermal::Fair);
        assert_eq!(from_android_status(3), Thermal::Serious);
        assert_eq!(from_android_status(6), Thermal::Critical);
        assert_eq!(from_apple_state(0), Thermal::Nominal);
        assert_eq!(from_apple_state(3), Thermal::Critical);
        assert!(Thermal::Critical > Thermal::Fair);
    }

    #[test]
    fn total_memory_comes_from_the_meminfo_line() {
        assert_eq!(
            meminfo_total("MemFree: 3 kB\nMemTotal:       16 kB\n"),
            Some(16 * 1024)
        );
        assert_eq!(meminfo_total("nothing"), None);
    }

    #[test]
    fn this_machine_has_memory() {
        assert!(device().total_memory > 0);
    }
}
