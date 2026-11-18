//! Game controllers: buttons by place, sticks and triggers, and connect and disconnect.
//!
//! The app polls [`Pads::poll`] on the thread that owns its event loop. Windows (Windows.Gaming.Input,
//! XInput when that does not activate), Linux (evdev) and macOS and iOS (GameController) have a
//! backend; other systems build an empty [`Pads`].

#[cfg(not(all(
    feature = "gamepad",
    any(windows, target_os = "linux", target_vendor = "apple")
)))]
mod off;
#[cfg(all(
    feature = "gamepad",
    any(windows, target_os = "linux", target_vendor = "apple")
))]
mod on;

#[cfg(not(all(
    feature = "gamepad",
    any(windows, target_os = "linux", target_vendor = "apple")
)))]
pub use off::Pads;
#[cfg(all(
    feature = "gamepad",
    any(windows, target_os = "linux", target_vendor = "apple")
))]
pub use on::Pads;

#[cfg(all(feature = "gamepad", target_vendor = "apple"))]
mod apple;
#[cfg(all(feature = "gamepad", target_os = "linux"))]
mod evdev;
#[cfg(all(
    feature = "gamepad",
    any(windows, target_os = "linux", target_vendor = "apple")
))]
mod state;
#[cfg(all(feature = "gamepad", windows))]
mod win;

/// A pad, for the session: its id is never given to another pad.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PadId(u32);

/// A connected pad, as [`Pads::list`] gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadInfo {
    /// The pad's id, the same in its events.
    pub id: PadId,
    /// The name the system gives, or a generic one when it gives none.
    pub name: String,
}

/// A change on a pad, from [`Pads::poll`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PadEvent {
    /// A pad was plugged in or found at start.
    Connected(PadId),
    /// A pad was unplugged or stopped answering.
    Disconnected(PadId),
    /// A button went down or up.
    Button {
        /// The pad.
        pad: PadId,
        /// The button, by its place on the pad.
        button: PadButton,
        /// True on press, false on release.
        down: bool,
    },
    /// An axis moved. Raw, no dead zone.
    Axis {
        /// The pad.
        pad: PadId,
        /// The axis.
        axis: PadAxis,
        /// Sticks from -1 to 1 with up positive; triggers from 0 to 1.
        value: f32,
    },
}

/// A button, by its place: the face buttons are south (bottom), east (right), west (left) and
/// north (top), whatever they are labelled.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadButton {
    /// Bottom face button (A on Xbox).
    South,
    /// Right face button (B on Xbox).
    East,
    /// Left face button (X on Xbox).
    West,
    /// Top face button (Y on Xbox).
    North,
    /// Left shoulder button.
    LeftBumper,
    /// Right shoulder button.
    RightBumper,
    /// Left stick pressed in.
    LeftStick,
    /// Right stick pressed in.
    RightStick,
    /// Back, view or select.
    Select,
    /// Start or menu.
    Start,
    /// The home or guide button.
    Guide,
    /// Directional pad, up.
    DpadUp,
    /// Directional pad, down.
    DpadDown,
    /// Directional pad, left.
    DpadLeft,
    /// Directional pad, right.
    DpadRight,
}

/// An analog axis.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadAxis {
    /// Left stick, right is positive.
    LeftX,
    /// Left stick, up is positive.
    LeftY,
    /// Right stick, right is positive.
    RightX,
    /// Right stick, up is positive.
    RightY,
    /// Left trigger, 0 at rest and 1 pulled.
    LeftTrigger,
    /// Right trigger, 0 at rest and 1 pulled.
    RightTrigger,
}

/// Whether pads are built in and polled on this platform.
pub(crate) const fn built() -> bool {
    cfg!(all(
        feature = "gamepad",
        any(windows, target_os = "linux", target_vendor = "apple")
    ))
}

#[cfg(test)]
mod tests {
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use super::Pads;

    #[test]
    #[ignore = "needs a pad plugged in; run with --ignored"]
    fn real_pads() {
        let mut pads = Pads::new();
        let mut events = Vec::new();
        let end = Instant::now() + Duration::from_secs(2);
        while Instant::now() < end {
            pads.poll(&mut events);
            sleep(Duration::from_millis(4));
        }
        println!("pads: {:?}", pads.list());
        for event in &events {
            println!("{event:?}");
        }
    }
}
