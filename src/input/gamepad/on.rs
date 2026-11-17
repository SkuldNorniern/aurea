//! Pads on Windows and Linux: the platform source reads each pad's state, and [`Hub`] turns the
//! changes into events.

use super::PadEvent;
use super::PadInfo;
use super::state::{Found, Hub};

#[cfg(target_os = "linux")]
use super::evdev::Source;
#[cfg(windows)]
use super::win::Source;

/// Game controllers, polled by the app on its event loop thread.
///
/// ```rust,no_run
/// use aurea::input::gamepad::{PadEvent, Pads};
///
/// let mut pads = Pads::new();
/// let mut events = Vec::new();
/// pads.poll(&mut events);
/// for event in events.drain(..) {
///     if let PadEvent::Button { button, down: true, .. } = event {
///         println!("{button:?}");
///     }
/// }
/// ```
pub struct Pads {
    hub: Hub,
    source: Source,
    found: Vec<Found>,
}

impl Pads {
    /// Starts looking for pads. Nothing is reported until [`Pads::poll`].
    pub fn new() -> Pads {
        Pads {
            hub: Hub::default(),
            source: Source::new(),
            found: Vec::new(),
        }
    }

    /// Reads every pad and appends the events since the last poll to `events`, connections
    /// included. Call it at the rate the app reads input, such as once a frame.
    pub fn poll(&mut self, events: &mut Vec<PadEvent>) {
        self.found.clear();
        self.source.read(&mut self.found);
        self.hub.update(&self.found, events);
    }

    /// The pads connected now, in the order they came.
    pub fn list(&self) -> &[PadInfo] {
        self.hub.list()
    }
}

impl Default for Pads {
    fn default() -> Pads {
        Pads::new()
    }
}
