//! Event queue for window-level events.

use aurea_foundation::{WindowEvent, lock};
use std::mem::{discriminant, take};
use std::sync::Mutex;

/// A window's pending native events.
///
/// Deliberately holds only events, not the callbacks that consume them: the
/// platform can deliver an event from a thread that is not the UI thread (a
/// render thread reporting a lost surface, for instance), so pushing has to be
/// thread-safe. Dispatching to application callbacks is a UI-thread concern and
/// lives in the `aurea` crate's thread-local registry.
pub struct EventQueue {
    events: Mutex<Vec<WindowEvent>>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    pub fn push(&self, event: WindowEvent) {
        let mut events = lock(&self.events);
        // Coalesce high-frequency motion events so a fast mouse or trackpad
        // never queues more than one entry per process_events() call. How they
        // merge depends on what the payload means: an absolute position is
        // replaced by the newest one, while deltas must be summed or the motion
        // they describe is silently thrown away.
        match &event {
            // With the cursor grabbed, every move comes with a raw delta, so
            // the two alternate and neither was ever next to its own kind.
            // Either merges past the other, but never past anything else.
            WindowEvent::MouseMove { .. } => {
                if let Some(last) = trailing_motion(&mut events)
                    .find(|e| discriminant(&**e) == discriminant(&event))
                {
                    *last = event;
                    return;
                }
            }
            WindowEvent::RawMouseMotion { delta_x, delta_y } => {
                if let Some(WindowEvent::RawMouseMotion {
                    delta_x: last_x,
                    delta_y: last_y,
                }) = trailing_motion(&mut events)
                    .find(|e| matches!(e, WindowEvent::RawMouseMotion { .. }))
                {
                    *last_x += delta_x;
                    *last_y += delta_y;
                    return;
                }
            }
            WindowEvent::MouseWheel {
                delta_x,
                delta_y,
                modifiers,
            } => {
                if let Some(WindowEvent::MouseWheel {
                    delta_x: last_x,
                    delta_y: last_y,
                    modifiers: last_modifiers,
                }) = events.last_mut()
                    // Only merge scrolls under the same modifiers: Ctrl+wheel is
                    // usually zoom, not scroll, and the two must stay distinct.
                    && *last_modifiers == *modifiers
                {
                    *last_x += delta_x;
                    *last_y += delta_y;
                    return;
                }
            }
            _ => {}
        }
        events.push(event);
    }

    pub fn pop_all(&self) -> Vec<WindowEvent> {
        let mut events = lock(&self.events);
        take(&mut *events)
    }
}

/// The run of pointer motion at the end of the queue, newest first.
fn trailing_motion(events: &mut [WindowEvent]) -> impl Iterator<Item = &mut WindowEvent> {
    events.iter_mut().rev().take_while(|e| {
        matches!(
            e,
            WindowEvent::MouseMove { .. } | WindowEvent::RawMouseMotion { .. }
        )
    })
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurea_foundation::{Modifiers, MouseButtons};

    fn wheel(delta_y: f64, modifiers: Modifiers) -> WindowEvent {
        WindowEvent::MouseWheel {
            delta_x: 0.0,
            delta_y,
            modifiers,
        }
    }

    #[test]
    fn wheel_deltas_accumulate() {
        let q = EventQueue::new();
        q.push(wheel(3.0, Modifiers::new()));
        q.push(wheel(4.0, Modifiers::new()));
        q.push(wheel(5.0, Modifiers::new()));

        let events = q.pop_all();
        assert_eq!(events.len(), 1);
        match events[0] {
            WindowEvent::MouseWheel { delta_y, .. } => assert!((delta_y - 12.0).abs() < 1e-9),
            ref other => panic!("expected MouseWheel, got {other:?}"),
        }
    }

    #[test]
    fn wheel_with_different_modifiers_is_not_merged() {
        let q = EventQueue::new();
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::new()
        };
        q.push(wheel(3.0, Modifiers::new()));
        q.push(wheel(4.0, ctrl));

        assert_eq!(q.pop_all().len(), 2);
    }

    fn motion(x: f64) -> WindowEvent {
        WindowEvent::MouseMove {
            x,
            y: 0.0,
            buttons: MouseButtons::default(),
            modifiers: Modifiers::new(),
        }
    }

    fn raw(dx: f64) -> WindowEvent {
        WindowEvent::RawMouseMotion {
            delta_x: dx,
            delta_y: 0.0,
        }
    }

    /// A grabbed cursor sends a move and a raw delta for every motion.
    #[test]
    fn alternating_moves_and_raw_motion_still_merge() {
        let q = EventQueue::new();
        for i in 0..1000 {
            q.push(motion(f64::from(i)));
            q.push(raw(1.0));
        }

        let events = q.pop_all();
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(matches!(events[0], WindowEvent::MouseMove { x, .. } if x == 999.0));
        assert!(
            matches!(events[1], WindowEvent::RawMouseMotion { delta_x, .. } if delta_x == 1000.0)
        );
    }

    /// A button in between keeps the motion on either side of it apart.
    #[test]
    fn motion_does_not_merge_past_other_events() {
        let q = EventQueue::new();
        q.push(motion(1.0));
        q.push(WindowEvent::Focused);
        q.push(motion(2.0));
        q.push(raw(1.0));
        q.push(motion(3.0));

        let events = q.pop_all();
        assert_eq!(events.len(), 4, "{events:?}");
        assert!(matches!(events[0], WindowEvent::MouseMove { x, .. } if x == 1.0));
        assert!(matches!(events[2], WindowEvent::MouseMove { x, .. } if x == 3.0));
    }

    #[test]
    fn raw_motion_deltas_accumulate() {
        let q = EventQueue::new();
        q.push(WindowEvent::RawMouseMotion {
            delta_x: 1.0,
            delta_y: -2.0,
        });
        q.push(WindowEvent::RawMouseMotion {
            delta_x: 4.0,
            delta_y: 2.0,
        });

        let events = q.pop_all();
        assert_eq!(events.len(), 1);
        match events[0] {
            WindowEvent::RawMouseMotion { delta_x, delta_y } => {
                assert!((delta_x - 5.0).abs() < 1e-9);
                assert!(delta_y.abs() < 1e-9);
            }
            ref other => panic!("expected RawMouseMotion, got {other:?}"),
        }
    }

    #[test]
    fn mouse_move_keeps_newest_position() {
        let q = EventQueue::new();
        q.push(WindowEvent::MouseMove {
            x: 1.0,
            y: 1.0,
            buttons: MouseButtons::default(),
            modifiers: Modifiers::default(),
        });
        q.push(WindowEvent::MouseMove {
            x: 9.0,
            y: 7.0,
            buttons: MouseButtons::default(),
            modifiers: Modifiers::default(),
        });

        let events = q.pop_all();
        assert_eq!(events.len(), 1);
        match events[0] {
            WindowEvent::MouseMove { x, y, .. } => {
                assert!((x - 9.0).abs() < 1e-9 && (y - 7.0).abs() < 1e-9);
            }
            ref other => panic!("expected MouseMove, got {other:?}"),
        }
    }
}
