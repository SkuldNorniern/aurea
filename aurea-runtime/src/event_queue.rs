//! Event queue for window-level events.

use aurea_foundation::{EventCallback, WindowEvent, lock};
use std::mem::{discriminant, take};
use std::sync::{Arc, Mutex};

pub struct EventQueue {
    events: Mutex<Vec<WindowEvent>>,
    callbacks: Mutex<Arc<Vec<EventCallback>>>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            callbacks: Mutex::new(Arc::new(Vec::new())),
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
            WindowEvent::MouseMove { .. } => {
                if let Some(last) = events.last_mut()
                    && discriminant(last) == discriminant(&event)
                {
                    *last = event;
                    return;
                }
            }
            WindowEvent::RawMouseMotion { delta_x, delta_y } => {
                if let Some(WindowEvent::RawMouseMotion {
                    delta_x: last_x,
                    delta_y: last_y,
                }) = events.last_mut()
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

    pub fn register_callback(&self, callback: EventCallback) {
        let mut callbacks = lock(&self.callbacks);
        let mut updated = (**callbacks).clone();
        updated.push(callback);
        *callbacks = Arc::new(updated);
    }

    pub fn process_events(&self) -> Vec<WindowEvent> {
        let events = self.pop_all();
        if events.is_empty() {
            return Vec::new();
        }

        // Cheap Arc clone instead of cloning the whole callback Vec; the lock
        // is still released before invoking callbacks (which may re-register).
        let callbacks = lock(&self.callbacks).clone();

        for event in &events {
            for callback in callbacks.iter() {
                callback(event.clone());
            }
        }

        events
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurea_foundation::Modifiers;

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
        q.push(WindowEvent::MouseMove { x: 1.0, y: 1.0 });
        q.push(WindowEvent::MouseMove { x: 9.0, y: 7.0 });

        let events = q.pop_all();
        assert_eq!(events.len(), 1);
        match events[0] {
            WindowEvent::MouseMove { x, y } => {
                assert!((x - 9.0).abs() < 1e-9 && (y - 7.0).abs() < 1e-9);
            }
            ref other => panic!("expected MouseMove, got {other:?}"),
        }
    }
}
