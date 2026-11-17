//! macOS and iOS: GameController's extended gamepads, read each poll by the native host.

use std::ffi::{CStr, c_char, c_int};

use super::state::{Found, Snapshot, trigger, unit};
use crate::input::gamepad::{PadAxis, PadButton};

/// `NGPadState` of `apple/gamepad.h`.
#[repr(C)]
struct PadState {
    key: usize,
    name: [c_char; 64],
    buttons: u32,
    axes: [f32; 6],
}

unsafe extern "C" {
    fn ng_apple_pads_read(out: *mut PadState, max: c_int) -> c_int;
}

/// The native bit order of `buttons`.
const PLACES: [PadButton; 15] = [
    PadButton::South,
    PadButton::East,
    PadButton::West,
    PadButton::North,
    PadButton::LeftBumper,
    PadButton::RightBumper,
    PadButton::LeftStick,
    PadButton::RightStick,
    PadButton::Select,
    PadButton::Start,
    PadButton::Guide,
    PadButton::DpadUp,
    PadButton::DpadDown,
    PadButton::DpadLeft,
    PadButton::DpadRight,
];

/// More than GameController takes at once.
const MAX_PADS: usize = 8;
const MAX_PADS_C: c_int = 8;

pub(super) struct Source {
    states: Vec<PadState>,
}

impl Source {
    pub(super) fn new() -> Source {
        Source {
            states: (0..MAX_PADS)
                .map(|_| PadState {
                    key: 0,
                    name: [0; 64],
                    buttons: 0,
                    axes: [0.0; 6],
                })
                .collect(),
        }
    }

    pub(super) fn read(&mut self, found: &mut Vec<Found>) {
        // SAFETY: the host writes at most MAX_PADS states into the buffer it is given.
        let count = unsafe { ng_apple_pads_read(self.states.as_mut_ptr(), MAX_PADS_C) };
        let count = usize::try_from(count).unwrap_or(0).min(MAX_PADS);
        for state in &self.states[..count] {
            found.push(Found {
                key: state.key,
                name: name(state),
                state: snapshot(state),
            });
        }
    }
}

fn name(state: &PadState) -> String {
    let mut bytes = state.name;
    bytes[bytes.len() - 1] = 0;
    // SAFETY: the last byte is a nul, so the scan stays in the array.
    unsafe { CStr::from_ptr(bytes.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn snapshot(state: &PadState) -> Snapshot {
    let mut snapshot = Snapshot::default();
    for (bit, button) in PLACES.into_iter().enumerate() {
        if state.buttons & (1 << bit) != 0 {
            snapshot.press(button);
        }
    }
    let [lx, ly, rx, ry, lt, rt] = state.axes.map(f64::from);
    snapshot.set_axis(PadAxis::LeftX, unit(lx));
    snapshot.set_axis(PadAxis::LeftY, unit(ly));
    snapshot.set_axis(PadAxis::RightX, unit(rx));
    snapshot.set_axis(PadAxis::RightY, unit(ry));
    snapshot.set_axis(PadAxis::LeftTrigger, trigger(lt));
    snapshot.set_axis(PadAxis::RightTrigger, trigger(rt));
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(buttons: u32, axes: [f32; 6]) -> PadState {
        let mut name = [0; 64];
        for (to, from) in name.iter_mut().zip(b"Pad\0") {
            *to = *from as c_char;
        }
        PadState {
            key: 7,
            name,
            buttons,
            axes,
        }
    }

    #[test]
    fn bits_land_on_their_places() {
        let snap = snapshot(&state(1 | 1 << 10 | 1 << 14, [0.0; 6]));
        assert!(snap.is_down(PadButton::South));
        assert!(snap.is_down(PadButton::Guide));
        assert!(snap.is_down(PadButton::DpadRight));
        assert!(!snap.is_down(PadButton::East));
    }

    #[test]
    fn axes_keep_up_positive_and_clamp() {
        let snap = snapshot(&state(0, [0.5, 1.0, -2.0, f32::NAN, 0.25, 3.0]));
        assert_eq!(snap.axis(PadAxis::LeftX), 0.5);
        assert_eq!(snap.axis(PadAxis::LeftY), 1.0);
        assert_eq!(snap.axis(PadAxis::RightX), -1.0);
        assert_eq!(snap.axis(PadAxis::RightY), 0.0);
        assert_eq!(snap.axis(PadAxis::LeftTrigger), 0.25);
        assert_eq!(snap.axis(PadAxis::RightTrigger), 1.0);
    }

    #[test]
    fn a_name_without_its_nul_still_ends() {
        let mut s = state(0, [0.0; 6]);
        s.name = [b'x' as c_char; 64];
        assert_eq!(name(&s).len(), 63);
    }
}
