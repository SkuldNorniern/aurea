//! Event queue for window-level events.

use aurea_foundation::{WindowEvent, lock};
use std::mem::{discriminant, take};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A window's pending native events.
pub struct EventQueue {
    events: Mutex<Vec<WindowEvent>>,
    pen_batch_size: AtomicUsize,
}

impl EventQueue {
    /// Creates an empty queue.
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            pen_batch_size: AtomicUsize::new(0),
        }
    }

    /// Batches new consecutive pen samples into events of at most 64 readings.
    /// Existing queued events are unchanged. Disabled by default for compatibility.
    pub fn set_pen_batching(&self, enabled: bool) {
        self.set_pen_batch_size(if enabled { 64 } else { 0 });
    }

    /// Maximum readings per newly queued pen batch, clamped to 256. Zero delivers
    /// individual `Pen` events. This groups readings without dropping or delaying
    /// them; a partial batch is delivered on the next queue drain.
    pub fn set_pen_batch_size(&self, size: usize) {
        self.pen_batch_size.store(size.min(256), Ordering::Release);
    }

    /// Current batch limit. Zero means individual pen events.
    pub fn pen_batch_size(&self) -> usize {
        self.pen_batch_size.load(Ordering::Acquire)
    }

    /// Queues `event`, merging it into a pending one of the same kind when possible.
    pub fn push(&self, event: WindowEvent) {
        let mut events = lock(&self.events);
        let pen_batch_size = self.pen_batch_size();
        if pen_batch_size > 0
            && let WindowEvent::Pen(sample) = event
        {
            if let Some(WindowEvent::PenBatch(samples)) = events.last_mut()
                && samples.len() < pen_batch_size
            {
                samples.push(sample);
            } else {
                events.push(WindowEvent::PenBatch(vec![sample]));
            }
            return;
        }
        match &event {
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
                    && *last_modifiers == *modifiers
                {
                    *last_x += delta_x;
                    *last_y += delta_y;
                    return;
                }
            }
            WindowEvent::Resized { .. } | WindowEvent::Moved { .. } => {
                if let Some(last) = events
                    .iter_mut()
                    .rev()
                    .take_while(|e| {
                        matches!(e, WindowEvent::Resized { .. } | WindowEvent::Moved { .. })
                    })
                    .find(|e| discriminant(&**e) == discriminant(&event))
                {
                    *last = event;
                    return;
                }
            }
            _ => {}
        }
        events.push(event);
    }

    /// Takes every queued event, leaving the queue empty.
    pub fn pop_all(&self) -> Vec<WindowEvent> {
        let mut events = lock(&self.events);
        take(&mut *events)
    }
}

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
    use aurea_foundation::{Modifiers, MouseButtons, PointerKind};
    use aurea_foundation::{PenPhase, PenSample};
    use std::time::Duration;

    #[test]
    fn pen_batch_limit_changes_without_waiting_for_a_full_batch() {
        let queue = EventQueue::new();
        assert_eq!(queue.pen_batch_size(), 0);
        queue.set_pen_batch_size(3);
        for i in 0..8 {
            queue.push(WindowEvent::Pen(PenSample {
                x: f64::from(i),
                ..PenSample::default()
            }));
        }
        let events = queue.pop_all();
        assert_eq!(
            events
                .iter()
                .map(|e| e.pen_samples().len())
                .collect::<Vec<_>>(),
            [3, 3, 2]
        );
        assert_eq!(
            events
                .iter()
                .flat_map(WindowEvent::pen_samples)
                .map(|s| s.x)
                .collect::<Vec<_>>(),
            (0..8).map(f64::from).collect::<Vec<_>>()
        );
    }

    #[test]
    fn changing_pen_batch_size_preserves_queued_readings() {
        let queue = EventQueue::new();
        queue.set_pen_batch_size(usize::MAX);
        assert_eq!(queue.pen_batch_size(), 256);
        queue.push(WindowEvent::Pen(PenSample::default()));
        queue.set_pen_batch_size(0);
        queue.push(WindowEvent::Pen(PenSample::default()));
        let events = queue.pop_all();
        assert!(matches!(&events[0], WindowEvent::PenBatch(samples) if samples.len() == 1));
        assert!(matches!(&events[1], WindowEvent::Pen(_)));
        queue.set_pen_batching(true);
        assert_eq!(queue.pen_batch_size(), 64);
    }

    #[test]
    fn pen_batches_preserve_every_reading_and_stop_at_input_boundaries() {
        let queue = EventQueue::new();
        queue.set_pen_batching(true);
        let samples: Vec<_> = (0..130u32)
            .map(|i| PenSample {
                x: f64::from(i) * 0.25,
                pressure: i as f32 / 130.0,
                time: Duration::from_micros(u64::from(i)),
                phase: if i == 0 {
                    PenPhase::Down
                } else if i == 129 {
                    PenPhase::Up
                } else {
                    PenPhase::Move
                },
                ..PenSample::default()
            })
            .collect();
        for sample in &samples[..65] {
            queue.push(WindowEvent::Pen(*sample));
        }
        queue.push(WindowEvent::Focused);
        for sample in &samples[65..] {
            queue.push(WindowEvent::Pen(*sample));
        }
        let events = queue.pop_all();
        assert_eq!(
            events
                .iter()
                .map(|e| e.pen_samples().len())
                .collect::<Vec<_>>(),
            [64, 1, 0, 64, 1]
        );
        assert!(matches!(events[2], WindowEvent::Focused));
        let got: Vec<_> = events
            .iter()
            .flat_map(|e| e.pen_samples().iter().copied())
            .collect();
        assert_eq!(got, samples);
        queue.set_pen_batching(false);
        queue.push(WindowEvent::Pen(samples[0]));
        queue.push(WindowEvent::Pen(samples[1]));
        assert!(
            queue
                .pop_all()
                .iter()
                .all(|e| matches!(e, WindowEvent::Pen(_)))
        );
    }

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
            time: Default::default(),
            pointer: PointerKind::Mouse,
        }
    }

    fn raw(dx: f64) -> WindowEvent {
        WindowEvent::RawMouseMotion {
            delta_x: dx,
            delta_y: 0.0,
        }
    }

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

    fn drag_left_edge(q: &EventQueue, steps: i32) {
        for step in 0..steps {
            q.push(WindowEvent::Moved { x: -step, y: 0 });
            q.push(WindowEvent::Resized {
                width: 800 + step.cast_unsigned(),
                height: 600,
            });
        }
    }

    #[test]
    fn a_live_resize_leaves_the_latest_size_and_position() {
        let q = EventQueue::new();
        drag_left_edge(&q, 500);
        q.push(WindowEvent::Focused);
        q.push(WindowEvent::Resized {
            width: 1,
            height: 1,
        });

        let events = q.pop_all();
        assert_eq!(events.len(), 4, "{events:?}");
        assert!(matches!(events[0], WindowEvent::Moved { x: -499, .. }));
        assert!(matches!(
            events[1],
            WindowEvent::Resized {
                width: 1299,
                height: 600
            }
        ));
        assert!(matches!(events[2], WindowEvent::Focused));
        assert!(matches!(events[3], WindowEvent::Resized { width: 1, .. }));
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
            time: Default::default(),
            pointer: PointerKind::Mouse,
        });
        q.push(WindowEvent::MouseMove {
            x: 9.0,
            y: 7.0,
            buttons: MouseButtons::default(),
            modifiers: Modifiers::default(),
            time: Default::default(),
            pointer: PointerKind::Mouse,
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
