//! Windows: Windows.Gaming.Input when it activates, XInput when it does not. XInput is loaded at
//! run time, so nothing is linked against it.

use std::mem::transmute;

use windows::Gaming::Input::{Gamepad, GamepadButtons};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::{Interface, s, w};

use super::state::{Found, Snapshot, trigger, unit};
use crate::input::gamepad::{PadAxis, PadButton};

type GetState = unsafe extern "system" fn(u32, *mut XInputState) -> u32;

/// Presses each button whose bit is set in `bits`, through `table`.
fn press_bits(snapshot: &mut Snapshot, bits: u32, table: &[(u32, PadButton)]) {
    for &(bit, button) in table {
        if bits & bit != 0 {
            snapshot.press(button);
        }
    }
}

/// `XINPUT_STATE`.
#[repr(C)]
#[derive(Default)]
pub(super) struct XInputState {
    packet: u32,
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    left_x: i16,
    left_y: i16,
    right_x: i16,
    right_y: i16,
}

/// Where pads come from: the WGI gamepads kept alive while their keys are in use, or XInput's
/// slots (`None` when the library is missing).
pub(super) enum Source {
    Wgi(Vec<Gamepad>),
    XInput(Option<GetState>),
}

impl Source {
    pub(super) fn new() -> Source {
        // SAFETY: a plain call. An error means COM is set up on this thread already, which is fine.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        match Gamepad::Gamepads() {
            Ok(_) => Source::Wgi(Vec::new()),
            Err(_) => Source::XInput(load_xinput()),
        }
    }

    pub(super) fn read(&mut self, found: &mut Vec<Found>) {
        match self {
            Source::Wgi(held) => *held = read_wgi(found),
            Source::XInput(get) => read_xinput(*get, found),
        }
    }
}

fn read_wgi(found: &mut Vec<Found>) -> Vec<Gamepad> {
    let mut held = Vec::new();
    let Ok(list) = Gamepad::Gamepads() else {
        return held;
    };
    let Ok(count) = list.Size() else {
        return held;
    };
    for at in 0..count {
        let Ok(pad) = list.GetAt(at) else {
            continue;
        };
        let Ok(reading) = pad.GetCurrentReading() else {
            continue;
        };
        found.push(Found {
            key: pad.as_raw().addr(),
            name: String::from("gamepad"),
            state: wgi_snapshot(
                reading.Buttons.0,
                reading.LeftTrigger,
                reading.RightTrigger,
                reading.LeftThumbstickX,
                reading.LeftThumbstickY,
                reading.RightThumbstickX,
                reading.RightThumbstickY,
            ),
        });
        held.push(pad);
    }
    held
}

fn read_xinput(get: Option<GetState>, found: &mut Vec<Found>) {
    let Some(get) = get else {
        return;
    };
    for slot in 0..4u32 {
        let mut state = XInputState::default();
        // SAFETY: `state` is a valid XINPUT_STATE, the only thing XInputGetState writes.
        if unsafe { get(slot, &mut state) } == 0 {
            found.push(Found {
                key: slot as usize,
                name: String::from("xinput gamepad"),
                state: xinput_snapshot(&state),
            });
        }
    }
}

/// The library stays loaded for the process.
fn load_xinput() -> Option<GetState> {
    // SAFETY: a fixed wide name; the library is only called through its export below.
    let module = unsafe { LoadLibraryW(w!("xinput1_4.dll")) }.ok()?;
    // SAFETY: a fixed C name.
    let proc = unsafe { GetProcAddress(module, s!("XInputGetState")) }?;
    // SAFETY: XInputGetState has this signature in every XInput version.
    Some(unsafe { transmute::<unsafe extern "system" fn() -> isize, GetState>(proc) })
}

/// A WGI reading as a snapshot. Its thumbsticks are read up positive, as XInput's are.
fn wgi_snapshot(
    buttons: u32,
    left_trigger: f64,
    right_trigger: f64,
    left_x: f64,
    left_y: f64,
    right_x: f64,
    right_y: f64,
) -> Snapshot {
    const MAP: [(u32, PadButton); 14] = [
        (GamepadButtons::Menu.0, PadButton::Start),
        (GamepadButtons::View.0, PadButton::Select),
        (GamepadButtons::A.0, PadButton::South),
        (GamepadButtons::B.0, PadButton::East),
        (GamepadButtons::X.0, PadButton::West),
        (GamepadButtons::Y.0, PadButton::North),
        (GamepadButtons::DPadUp.0, PadButton::DpadUp),
        (GamepadButtons::DPadDown.0, PadButton::DpadDown),
        (GamepadButtons::DPadLeft.0, PadButton::DpadLeft),
        (GamepadButtons::DPadRight.0, PadButton::DpadRight),
        (GamepadButtons::LeftShoulder.0, PadButton::LeftBumper),
        (GamepadButtons::RightShoulder.0, PadButton::RightBumper),
        (GamepadButtons::LeftThumbstick.0, PadButton::LeftStick),
        (GamepadButtons::RightThumbstick.0, PadButton::RightStick),
    ];
    let mut s = Snapshot::default();
    press_bits(&mut s, buttons, &MAP);
    s.set_axis(PadAxis::LeftTrigger, trigger(left_trigger));
    s.set_axis(PadAxis::RightTrigger, trigger(right_trigger));
    s.set_axis(PadAxis::LeftX, unit(left_x));
    s.set_axis(PadAxis::LeftY, unit(left_y));
    s.set_axis(PadAxis::RightX, unit(right_x));
    s.set_axis(PadAxis::RightY, unit(right_y));
    s
}

/// An XInput state as a snapshot. Sticks are raw signed values with up positive; triggers are
/// bytes.
fn xinput_snapshot(state: &XInputState) -> Snapshot {
    const MAP: [(u32, PadButton); 14] = [
        (0x0001, PadButton::DpadUp),
        (0x0002, PadButton::DpadDown),
        (0x0004, PadButton::DpadLeft),
        (0x0008, PadButton::DpadRight),
        (0x0010, PadButton::Start),
        (0x0020, PadButton::Select),
        (0x0040, PadButton::LeftStick),
        (0x0080, PadButton::RightStick),
        (0x0100, PadButton::LeftBumper),
        (0x0200, PadButton::RightBumper),
        (0x1000, PadButton::South),
        (0x2000, PadButton::East),
        (0x4000, PadButton::West),
        (0x8000, PadButton::North),
    ];
    let mut s = Snapshot::default();
    press_bits(&mut s, u32::from(state.buttons), &MAP);
    s.set_axis(
        PadAxis::LeftTrigger,
        trigger(f64::from(state.left_trigger) / 255.0),
    );
    s.set_axis(
        PadAxis::RightTrigger,
        trigger(f64::from(state.right_trigger) / 255.0),
    );
    s.set_axis(PadAxis::LeftX, unit(f64::from(state.left_x) / 32767.0));
    s.set_axis(PadAxis::LeftY, unit(f64::from(state.left_y) / 32767.0));
    s.set_axis(PadAxis::RightX, unit(f64::from(state.right_x) / 32767.0));
    s.set_axis(PadAxis::RightY, unit(f64::from(state.right_y) / 32767.0));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wgi_face_buttons_by_place() {
        let s = wgi_snapshot(
            GamepadButtons::A.0 | GamepadButtons::X.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        assert!(s.is_down(PadButton::South));
        assert!(s.is_down(PadButton::West));
        assert!(!s.is_down(PadButton::East));
        assert!(!s.is_down(PadButton::North));
    }

    #[test]
    fn wgi_menu_view_and_dpad() {
        let bits = GamepadButtons::Menu.0
            | GamepadButtons::View.0
            | GamepadButtons::DPadUp.0
            | GamepadButtons::RightShoulder.0;
        let s = wgi_snapshot(bits, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(s.is_down(PadButton::Start));
        assert!(s.is_down(PadButton::Select));
        assert!(s.is_down(PadButton::DpadUp));
        assert!(s.is_down(PadButton::RightBumper));
        assert!(!s.is_down(PadButton::Guide));
    }

    #[test]
    fn wgi_triggers_and_sticks() {
        let s = wgi_snapshot(0, 1.0, 2.0, -2.0, 0.5, 0.0, -0.25);
        assert_eq!(s.axis(PadAxis::LeftTrigger), 1.0);
        assert_eq!(s.axis(PadAxis::RightTrigger), 1.0);
        assert_eq!(s.axis(PadAxis::LeftX), -1.0);
        assert_eq!(s.axis(PadAxis::LeftY), 0.5);
        assert_eq!(s.axis(PadAxis::RightY), -0.25);
    }

    #[test]
    fn xinput_buttons_by_place() {
        let state = XInputState {
            buttons: 0x1000 | 0x2000 | 0x4000 | 0x8000,
            ..Default::default()
        };
        let s = xinput_snapshot(&state);
        assert!(s.is_down(PadButton::South));
        assert!(s.is_down(PadButton::East));
        assert!(s.is_down(PadButton::West));
        assert!(s.is_down(PadButton::North));
    }

    #[test]
    fn xinput_start_back_and_shoulders() {
        let state = XInputState {
            buttons: 0x0010 | 0x0020 | 0x0100,
            ..Default::default()
        };
        let s = xinput_snapshot(&state);
        assert!(s.is_down(PadButton::Start));
        assert!(s.is_down(PadButton::Select));
        assert!(s.is_down(PadButton::LeftBumper));
        assert!(!s.is_down(PadButton::RightBumper));
    }

    #[test]
    fn xinput_stick_up_is_positive_and_ranges_normalize() {
        let up = XInputState {
            left_y: 32767,
            right_y: -32768,
            ..Default::default()
        };
        let s = xinput_snapshot(&up);
        assert_eq!(s.axis(PadAxis::LeftY), 1.0);
        assert_eq!(s.axis(PadAxis::RightY), -1.0);
        assert_eq!(s.axis(PadAxis::LeftX), 0.0);
    }

    #[test]
    fn xinput_triggers_scale_to_one() {
        let state = XInputState {
            left_trigger: 255,
            right_trigger: 0,
            ..Default::default()
        };
        let s = xinput_snapshot(&state);
        assert_eq!(s.axis(PadAxis::LeftTrigger), 1.0);
        assert_eq!(s.axis(PadAxis::RightTrigger), 0.0);
    }
}
