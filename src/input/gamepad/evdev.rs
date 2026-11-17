//! Linux: pads from `/dev/input/event*`, read with evdev directly, without udev. New and removed
//! nodes come from inotify on `/dev/input`, and a node that cannot be opened (permissions) is
//! skipped until the next change there.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::mem::size_of;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use libc::{
    IN_ATTRIB, IN_CLOEXEC, IN_CREATE, IN_DELETE, IN_MOVED_FROM, IN_MOVED_TO, IN_NONBLOCK, Ioctl,
    O_CLOEXEC, O_NONBLOCK, inotify_add_watch, inotify_init1, ioctl, timeval,
};

use super::state::{Found, Snapshot, trigger, unit};
use crate::input::gamepad::{PadAxis, PadButton};

const INPUT_DIR: &str = "/dev/input";

const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_Z: u16 = 0x02;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;
const ABS_RZ: u16 = 0x05;
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;

/// The axes read, in the order of `Device::abs`: sticks, triggers, then the d-pad hat.
const ABS_SLOTS: [u16; 8] = [
    ABS_X, ABS_Y, ABS_Z, ABS_RX, ABS_RY, ABS_RZ, ABS_HAT0X, ABS_HAT0Y,
];

const BTN_SOUTH: u16 = 0x130;

/// Key codes and the button each presses. The face buttons follow the kernel's gamepad names
/// (BTN_NORTH and BTN_WEST), not the labels some drivers send.
const KEY_BUTTONS: [(u16, PadButton); 15] = [
    (BTN_SOUTH, PadButton::South),
    (0x131, PadButton::East),
    (0x133, PadButton::North),
    (0x134, PadButton::West),
    (0x136, PadButton::LeftBumper),
    (0x137, PadButton::RightBumper),
    (0x13d, PadButton::LeftStick),
    (0x13e, PadButton::RightStick),
    (0x13a, PadButton::Select),
    (0x13b, PadButton::Start),
    (0x13c, PadButton::Guide),
    (0x220, PadButton::DpadUp),
    (0x221, PadButton::DpadDown),
    (0x222, PadButton::DpadLeft),
    (0x223, PadButton::DpadRight),
];

const IOC_READ: u32 = 2;
const IOC_TYPE_EVDEV: u32 = 0x45;
const KEY_LEN: u32 = 96;
const ABS_BITS_LEN: u32 = 8;
const NAME_LEN: u32 = 256;
const ABSINFO_LEN: u32 = 24;

/// The `_IOC` request for an evdev read of `size` bytes.
const fn eviocg(nr: u32, size: u32) -> u32 {
    (IOC_READ << 30) | (size << 16) | (IOC_TYPE_EVDEV << 8) | nr
}

type Range = Option<(i32, i32)>;

const TIME_LEN: usize = size_of::<timeval>();
/// One `input_event`: time, type, code and value.
const EVENT_LEN: usize = TIME_LEN + 8;

/// A gamepad node, open, with its state kept up to date from its events.
struct Device {
    key: usize,
    path: PathBuf,
    file: File,
    name: String,
    keys: [u8; KEY_LEN as usize],
    abs: [i32; 8],
    ranges: [Range; 8],
}

impl Device {
    /// Reads what the device sent. False when it is gone.
    fn drain(&mut self) -> bool {
        let mut buf = [0u8; EVENT_LEN * 32];
        loop {
            match self.file.read(&mut buf) {
                Ok(0) => return false,
                Ok(n) => {
                    for event in buf[..n].as_chunks::<EVENT_LEN>().0 {
                        self.apply(event);
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return true,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
    }

    fn apply(&mut self, event: &[u8]) {
        let kind = u16::from_ne_bytes([event[TIME_LEN], event[TIME_LEN + 1]]);
        let code = u16::from_ne_bytes([event[TIME_LEN + 2], event[TIME_LEN + 3]]);
        let value = int_at(event, TIME_LEN + 4);
        match kind {
            // 2 is key repeat, which is not a change.
            EV_KEY if value == 0 || value == 1 => set_bit(&mut self.keys, code, value == 1),
            EV_ABS => {
                if let Some(slot) = abs_slot(code) {
                    self.abs[slot] = value;
                }
            }
            _ => {}
        }
    }
}

/// The gamepad nodes under `/dev/input`, and the inotify watch that says when they change.
pub(super) struct Source {
    inotify: Option<File>,
    devices: Vec<Device>,
    next_key: usize,
    dirty: bool,
}

impl Source {
    pub(super) fn new() -> Source {
        Source {
            inotify: watch_input_dir(),
            devices: Vec::new(),
            next_key: 0,
            dirty: true,
        }
    }

    pub(super) fn read(&mut self, found: &mut Vec<Found>) {
        if self.drain_inotify() {
            self.dirty = true;
        }
        if self.dirty {
            self.dirty = false;
            self.rescan();
        }
        self.devices.retain_mut(Device::drain);
        found.extend(self.devices.iter().map(|d| Found {
            key: d.key,
            name: d.name.clone(),
            state: snapshot(&d.keys, &d.abs, &d.ranges),
        }));
    }

    /// Reads the watch, returning whether anything in `/dev/input` changed.
    fn drain_inotify(&mut self) -> bool {
        let Some(file) = self.inotify.as_mut() else {
            return false;
        };
        let mut buf = [0u8; 4096];
        let mut changed = false;
        while let Ok(n) = file.read(&mut buf) {
            if n == 0 {
                break;
            }
            changed = true;
        }
        changed
    }

    /// Drops the nodes that went away and opens the new gamepad nodes.
    fn rescan(&mut self) {
        let present = event_nodes();
        self.devices.retain(|d| present.contains(&d.path));
        for path in present {
            if self.devices.iter().any(|d| d.path == path) {
                continue;
            }
            let key = self.next_key;
            self.next_key += 1;
            if let Some(device) = open(&path, key) {
                self.devices.push(device);
            }
        }
    }
}

/// The `event*` nodes in `/dev/input`. Empty when there is no such folder, as on WSL.
fn event_nodes() -> Vec<PathBuf> {
    let Ok(dir) = fs::read_dir(INPUT_DIR) else {
        return Vec::new();
    };
    dir.flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("event"))
        })
        .collect()
}

/// An inotify watch on `/dev/input`, or `None` when it cannot be set up.
fn watch_input_dir() -> Option<File> {
    // SAFETY: no pointers are passed; the result is checked below.
    let fd = unsafe { inotify_init1(IN_NONBLOCK | IN_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    // SAFETY: fd is a new descriptor that nothing else owns.
    let inotify = unsafe { OwnedFd::from_raw_fd(fd) };
    let mask = IN_CREATE | IN_DELETE | IN_ATTRIB | IN_MOVED_TO | IN_MOVED_FROM;
    // SAFETY: the path is a C string literal and fd is open.
    let watch = unsafe { inotify_add_watch(fd, c"/dev/input".as_ptr(), mask) };
    if watch < 0 {
        return None;
    }
    Some(File::from(inotify))
}

/// Opens `path` if it is a gamepad this process can read.
fn open(path: &Path, key: usize) -> Option<Device> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK | O_CLOEXEC)
        .open(path)
        .ok()?;
    let mut key_caps = [0u8; KEY_LEN as usize];
    let mut abs_caps = [0u8; ABS_BITS_LEN as usize];
    if !query(
        &file,
        eviocg(0x20 + u32::from(EV_KEY), KEY_LEN),
        &mut key_caps,
    ) || !query(
        &file,
        eviocg(0x20 + u32::from(EV_ABS), ABS_BITS_LEN),
        &mut abs_caps,
    ) || !is_gamepad(&key_caps, &abs_caps)
    {
        return None;
    }

    let mut keys = [0u8; KEY_LEN as usize];
    query(&file, eviocg(0x18, KEY_LEN), &mut keys);
    let mut name_buf = [0u8; NAME_LEN as usize];
    let name = if query(&file, eviocg(0x06, NAME_LEN), &mut name_buf) {
        text(&name_buf)
    } else {
        String::from("gamepad")
    };

    let mut abs = [0i32; 8];
    let mut ranges: [Range; 8] = [None; 8];
    for (slot, &code) in ABS_SLOTS.iter().enumerate() {
        if !bit(&abs_caps, code) {
            continue;
        }
        let mut info = [0u8; ABSINFO_LEN as usize];
        if query(
            &file,
            eviocg(0x40 + u32::from(code), ABSINFO_LEN),
            &mut info,
        ) {
            abs[slot] = int_at(&info, 0);
            ranges[slot] = Some((int_at(&info, 4), int_at(&info, 8)));
        }
    }

    Some(Device {
        key,
        path: path.to_path_buf(),
        file,
        name,
        keys,
        abs,
        ranges,
    })
}

/// Runs an evdev query into `buf`. False when the device does not answer it.
// The request is a u32; glibc's `Ioctl` is wider, so this is a lossless widening there.
#[allow(clippy::cast_lossless)]
fn query(file: &File, request: u32, buf: &mut [u8]) -> bool {
    // SAFETY: the buffer is as long as the request asks for, and the fd belongs to `file`.
    unsafe { ioctl(file.as_raw_fd(), request as Ioctl, buf.as_mut_ptr()) >= 0 }
}

fn text(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).trim().to_owned()
}

fn int_at(buf: &[u8], at: usize) -> i32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&buf[at..at + 4]);
    i32::from_ne_bytes(word)
}

fn bit(bits: &[u8], code: u16) -> bool {
    bits.get(usize::from(code / 8))
        .is_some_and(|byte| byte & (1u8 << (code % 8)) != 0)
}

fn set_bit(bits: &mut [u8], code: u16, on: bool) {
    if let Some(byte) = bits.get_mut(usize::from(code / 8)) {
        let mask = 1u8 << (code % 8);
        if on {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }
    }
}

/// Whether a node reports a gamepad: the bottom face button and the left stick's X axis.
fn is_gamepad(keys: &[u8], abs_caps: &[u8]) -> bool {
    bit(keys, BTN_SOUTH) && bit(abs_caps, ABS_X)
}

fn abs_slot(code: u16) -> Option<usize> {
    ABS_SLOTS.iter().position(|&slot| slot == code)
}

/// A device's state as a snapshot.
fn snapshot(keys: &[u8], abs: &[i32; 8], ranges: &[Range; 8]) -> Snapshot {
    let mut s = Snapshot::default();
    for (code, button) in KEY_BUTTONS {
        if bit(keys, code) {
            s.press(button);
        }
    }
    // The hat: -1 is left and up, +1 is right and down.
    if abs[6] < 0 {
        s.press(PadButton::DpadLeft);
    }
    if abs[6] > 0 {
        s.press(PadButton::DpadRight);
    }
    if abs[7] < 0 {
        s.press(PadButton::DpadUp);
    }
    if abs[7] > 0 {
        s.press(PadButton::DpadDown);
    }
    s.set_axis(PadAxis::LeftX, unit(stick(abs[0], ranges[0], false)));
    s.set_axis(PadAxis::LeftY, unit(stick(abs[1], ranges[1], true)));
    s.set_axis(PadAxis::LeftTrigger, trigger(pull(abs[2], ranges[2])));
    s.set_axis(PadAxis::RightX, unit(stick(abs[3], ranges[3], false)));
    s.set_axis(PadAxis::RightY, unit(stick(abs[4], ranges[4], true)));
    s.set_axis(PadAxis::RightTrigger, trigger(pull(abs[5], ranges[5])));
    s
}

/// A stick value from its range to -1..1. evdev counts Y down, so `invert` for Y.
fn stick(value: i32, range: Range, invert: bool) -> f64 {
    let Some((min, max)) = range else {
        return 0.0;
    };
    if max <= min {
        return 0.0;
    }
    let t = 2.0 * (f64::from(value) - f64::from(min)) / (f64::from(max) - f64::from(min)) - 1.0;
    if invert { -t } else { t }
}

/// A trigger value from its range to 0..1.
fn pull(value: i32, range: Range) -> f64 {
    let Some((min, max)) = range else {
        return 0.0;
    };
    if max <= min {
        return 0.0;
    }
    (f64::from(value) - f64::from(min)) / (f64::from(max) - f64::from(min))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: Range = Some((-32768, 32767));
    const TRIGGER: Range = Some((0, 255));

    fn keys_with(codes: &[u16]) -> [u8; KEY_LEN as usize] {
        let mut keys = [0u8; KEY_LEN as usize];
        for &code in codes {
            set_bit(&mut keys, code, true);
        }
        keys
    }

    fn resting() -> ([i32; 8], [Range; 8]) {
        let mut ranges = [None; 8];
        ranges[0] = FULL;
        ranges[1] = FULL;
        ranges[2] = TRIGGER;
        ranges[3] = FULL;
        ranges[4] = FULL;
        ranges[5] = TRIGGER;
        ([0; 8], ranges)
    }

    #[test]
    fn stick_y_is_up_positive() {
        let (mut abs, ranges) = resting();
        abs[1] = -32768;
        let s = snapshot(&keys_with(&[]), &abs, &ranges);
        assert_eq!(s.axis(PadAxis::LeftY), 1.0);
        abs[1] = 32767;
        let s = snapshot(&keys_with(&[]), &abs, &ranges);
        assert_eq!(s.axis(PadAxis::LeftY), -1.0);
    }

    #[test]
    fn stick_x_and_right_stick_range() {
        let (mut abs, ranges) = resting();
        abs[0] = 32767;
        abs[4] = -32768;
        let s = snapshot(&keys_with(&[]), &abs, &ranges);
        assert_eq!(s.axis(PadAxis::LeftX), 1.0);
        assert_eq!(s.axis(PadAxis::RightY), 1.0);
        assert!(
            s.axis(PadAxis::RightX).abs() < 1e-3,
            "centred stick reads near zero"
        );
    }

    #[test]
    fn trigger_range_maps_to_unit() {
        let (mut abs, ranges) = resting();
        abs[2] = 255;
        abs[5] = 0;
        let s = snapshot(&keys_with(&[]), &abs, &ranges);
        assert_eq!(s.axis(PadAxis::LeftTrigger), 1.0);
        assert_eq!(s.axis(PadAxis::RightTrigger), 0.0);
    }

    #[test]
    fn missing_range_reads_as_rest() {
        let (mut abs, _) = resting();
        abs[0] = 1000;
        let s = snapshot(&keys_with(&[]), &abs, &[None; 8]);
        assert_eq!(s.axis(PadAxis::LeftX), 0.0);
    }

    #[test]
    fn hat_becomes_dpad_buttons() {
        let (mut abs, ranges) = resting();
        abs[6] = -1;
        abs[7] = -1;
        let s = snapshot(&keys_with(&[]), &abs, &ranges);
        assert!(s.is_down(PadButton::DpadLeft));
        assert!(s.is_down(PadButton::DpadUp));
        assert!(!s.is_down(PadButton::DpadRight));
        assert!(!s.is_down(PadButton::DpadDown));

        abs[6] = 1;
        abs[7] = 1;
        let s = snapshot(&keys_with(&[]), &abs, &ranges);
        assert!(s.is_down(PadButton::DpadRight));
        assert!(s.is_down(PadButton::DpadDown));
        assert!(!s.is_down(PadButton::DpadLeft));
    }

    #[test]
    fn key_codes_map_to_places() {
        let (abs, ranges) = resting();
        let s = snapshot(
            &keys_with(&[BTN_SOUTH, 0x133, 0x13b, 0x13c, 0x136]),
            &abs,
            &ranges,
        );
        assert!(s.is_down(PadButton::South));
        assert!(s.is_down(PadButton::North));
        assert!(s.is_down(PadButton::Start));
        assert!(s.is_down(PadButton::Guide));
        assert!(s.is_down(PadButton::LeftBumper));
        assert!(!s.is_down(PadButton::East));
    }

    #[test]
    fn dpad_key_codes_map_too() {
        let (abs, ranges) = resting();
        let s = snapshot(&keys_with(&[0x220, 0x223]), &abs, &ranges);
        assert!(s.is_down(PadButton::DpadUp));
        assert!(s.is_down(PadButton::DpadRight));
    }

    #[test]
    fn keyboard_is_not_a_gamepad() {
        let mut abs_caps = [0u8; ABS_BITS_LEN as usize];
        set_bit(&mut abs_caps, ABS_X, true);
        assert!(!is_gamepad(&keys_with(&[0x110]), &abs_caps));
        assert!(is_gamepad(&keys_with(&[BTN_SOUTH]), &abs_caps));
        assert!(!is_gamepad(&keys_with(&[BTN_SOUTH]), &[0u8; 8]));
    }

    #[test]
    fn abs_codes_map_to_slots() {
        assert_eq!(abs_slot(ABS_X), Some(0));
        assert_eq!(abs_slot(ABS_RZ), Some(5));
        assert_eq!(abs_slot(ABS_HAT0Y), Some(7));
        assert_eq!(abs_slot(0x06), None);
    }

    #[test]
    fn key_bits_round_trip() {
        let mut keys = [0u8; KEY_LEN as usize];
        set_bit(&mut keys, 0x130, true);
        assert!(bit(&keys, 0x130));
        set_bit(&mut keys, 0x130, false);
        assert!(!bit(&keys, 0x130));
    }
}
