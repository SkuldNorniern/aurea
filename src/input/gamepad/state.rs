//! Pad state as snapshots. A backend builds a snapshot per pad each poll; [`Hub`] turns the changes
//! into events and the pads that come and go into `Connected` and `Disconnected`.

use super::{PadAxis, PadButton, PadEvent, PadId, PadInfo};

const BUTTONS: [PadButton; 15] = [
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

const AXES: [PadAxis; 6] = [
    PadAxis::LeftX,
    PadAxis::LeftY,
    PadAxis::RightX,
    PadAxis::RightY,
    PadAxis::LeftTrigger,
    PadAxis::RightTrigger,
];

fn button_index(button: PadButton) -> u32 {
    match button {
        PadButton::South => 0,
        PadButton::East => 1,
        PadButton::West => 2,
        PadButton::North => 3,
        PadButton::LeftBumper => 4,
        PadButton::RightBumper => 5,
        PadButton::LeftStick => 6,
        PadButton::RightStick => 7,
        PadButton::Select => 8,
        PadButton::Start => 9,
        PadButton::Guide => 10,
        PadButton::DpadUp => 11,
        PadButton::DpadDown => 12,
        PadButton::DpadLeft => 13,
        PadButton::DpadRight => 14,
    }
}

fn axis_index(axis: PadAxis) -> usize {
    match axis {
        PadAxis::LeftX => 0,
        PadAxis::LeftY => 1,
        PadAxis::RightX => 2,
        PadAxis::RightY => 3,
        PadAxis::LeftTrigger => 4,
        PadAxis::RightTrigger => 5,
    }
}

/// The buttons down and the axes of one pad. All zero and up is the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Snapshot {
    down: u32,
    axes: [f32; 6],
}

impl Snapshot {
    pub(crate) fn press(&mut self, button: PadButton) {
        self.down |= 1u32 << button_index(button);
    }

    pub(crate) fn set_axis(&mut self, axis: PadAxis, value: f32) {
        self.axes[axis_index(axis)] = value;
    }

    pub(crate) fn is_down(&self, button: PadButton) -> bool {
        self.down & (1u32 << button_index(button)) != 0
    }

    pub(crate) fn axis(&self, axis: PadAxis) -> f32 {
        self.axes[axis_index(axis)]
    }
}

/// A stick value, clamped to -1..1. NaN reads as centred.
pub(crate) fn unit(value: f64) -> f32 {
    narrow(if value.is_nan() {
        0.0
    } else {
        value.clamp(-1.0, 1.0)
    })
}

/// A trigger value, clamped to 0..1. NaN reads as released.
pub(crate) fn trigger(value: f64) -> f32 {
    narrow(if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    })
}

// Callers clamp first, so the narrowing keeps the value.
#[allow(clippy::cast_possible_truncation)]
fn narrow(value: f64) -> f32 {
    value as f32
}

/// Events for what changed from `prev` to `next` on `pad`.
pub(crate) fn diff(pad: PadId, prev: &Snapshot, next: &Snapshot, out: &mut Vec<PadEvent>) {
    for button in BUTTONS {
        let down = next.is_down(button);
        if down != prev.is_down(button) {
            out.push(PadEvent::Button { pad, button, down });
        }
    }
    for axis in AXES {
        let value = next.axis(axis);
        if value != prev.axis(axis) {
            out.push(PadEvent::Axis { pad, axis, value });
        }
    }
}

/// One pad as a backend found it this poll.
pub(crate) struct Found {
    /// Stable for as long as the pad stays connected, and never the same as another pad's.
    pub(crate) key: usize,
    pub(crate) name: String,
    pub(crate) state: Snapshot,
}

struct Slot {
    key: usize,
    last: Snapshot,
}

/// The pads known so far, with their ids. Each new pad gets the next id; ids are not reused.
#[derive(Default)]
pub(crate) struct Hub {
    slots: Vec<Slot>,
    list: Vec<PadInfo>,
    next: u32,
}

impl Hub {
    pub(crate) fn list(&self) -> &[PadInfo] {
        &self.list
    }

    /// Matches `found` against the known pads: disconnects the missing ones, connects the new
    /// ones and reports the changes of the rest.
    pub(crate) fn update(&mut self, found: &[Found], events: &mut Vec<PadEvent>) {
        let mut at = 0;
        while at < self.slots.len() {
            let key = self.slots[at].key;
            if found.iter().any(|f| f.key == key) {
                at += 1;
                continue;
            }
            events.push(PadEvent::Disconnected(self.list[at].id));
            self.slots.remove(at);
            self.list.remove(at);
        }
        for pad in found {
            match self.slots.iter().position(|s| s.key == pad.key) {
                Some(at) => {
                    diff(self.list[at].id, &self.slots[at].last, &pad.state, events);
                    self.slots[at].last = pad.state;
                }
                None => {
                    let id = PadId(self.next);
                    self.next += 1;
                    events.push(PadEvent::Connected(id));
                    diff(id, &Snapshot::default(), &pad.state, events);
                    self.slots.push(Slot {
                        key: pad.key,
                        last: pad.state,
                    });
                    self.list.push(PadInfo {
                        id,
                        name: pad.name.clone(),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressed(button: PadButton) -> Snapshot {
        let mut s = Snapshot::default();
        s.press(button);
        s
    }

    fn found(key: usize, state: Snapshot) -> Found {
        Found {
            key,
            name: format!("pad {key}"),
            state,
        }
    }

    #[test]
    fn button_edges_become_events() {
        let pad = PadId(7);
        let mut out = Vec::new();
        diff(
            pad,
            &Snapshot::default(),
            &pressed(PadButton::South),
            &mut out,
        );
        assert_eq!(
            out,
            [PadEvent::Button {
                pad,
                button: PadButton::South,
                down: true
            }]
        );
        out.clear();
        diff(
            pad,
            &pressed(PadButton::South),
            &pressed(PadButton::South),
            &mut out,
        );
        assert!(out.is_empty(), "a held button sends nothing");
        diff(
            pad,
            &pressed(PadButton::South),
            &Snapshot::default(),
            &mut out,
        );
        assert_eq!(
            out,
            [PadEvent::Button {
                pad,
                button: PadButton::South,
                down: false
            }]
        );
    }

    #[test]
    fn axis_changes_become_events() {
        let pad = PadId(1);
        let mut next = Snapshot::default();
        next.set_axis(PadAxis::RightTrigger, 0.5);
        let mut out = Vec::new();
        diff(pad, &Snapshot::default(), &next, &mut out);
        assert_eq!(
            out,
            [PadEvent::Axis {
                pad,
                axis: PadAxis::RightTrigger,
                value: 0.5
            }]
        );
    }

    #[test]
    fn hub_connects_diffs_and_disconnects() {
        let mut hub = Hub::default();
        let mut events = Vec::new();

        hub.update(&[found(10, pressed(PadButton::Start))], &mut events);
        assert_eq!(hub.list().len(), 1);
        let first = hub.list()[0].id;
        assert_eq!(
            events,
            [
                PadEvent::Connected(first),
                PadEvent::Button {
                    pad: first,
                    button: PadButton::Start,
                    down: true
                },
            ]
        );

        events.clear();
        hub.update(&[found(10, Snapshot::default())], &mut events);
        assert_eq!(
            events,
            [PadEvent::Button {
                pad: first,
                button: PadButton::Start,
                down: false
            }]
        );

        events.clear();
        hub.update(&[], &mut events);
        assert_eq!(events, [PadEvent::Disconnected(first)]);
        assert!(hub.list().is_empty());

        events.clear();
        hub.update(&[found(11, Snapshot::default())], &mut events);
        let second = hub.list()[0].id;
        assert_ne!(first, second, "ids are not reused");
        assert_eq!(events, [PadEvent::Connected(second)]);
        assert_eq!(hub.list()[0].name, "pad 11");
    }

    #[test]
    fn hub_keeps_ids_of_pads_still_present() {
        let mut hub = Hub::default();
        let mut events = Vec::new();
        hub.update(
            &[found(1, Snapshot::default()), found(2, Snapshot::default())],
            &mut events,
        );
        let ids: Vec<PadId> = hub.list().iter().map(|p| p.id).collect();
        events.clear();
        hub.update(&[found(2, Snapshot::default())], &mut events);
        assert_eq!(events, [PadEvent::Disconnected(ids[0])]);
        assert_eq!(hub.list()[0].id, ids[1]);
    }

    #[test]
    fn unit_and_trigger_clamp() {
        assert_eq!(unit(2.0), 1.0);
        assert_eq!(unit(-2.0), -1.0);
        assert_eq!(unit(f64::NAN), 0.0);
        assert_eq!(trigger(-0.5), 0.0);
        assert_eq!(trigger(1.5), 1.0);
    }
}
