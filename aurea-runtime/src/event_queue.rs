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
        // Coalesce high-frequency motion events: replace the tail if it is the
        // same variant, so a fast mouse or trackpad never queues more than one
        // entry per process_events() call.
        match &event {
            WindowEvent::MouseMove { .. }
            | WindowEvent::RawMouseMotion { .. }
            | WindowEvent::MouseWheel { .. } => {
                if let Some(last) = events.last_mut()
                    && discriminant(last) == discriminant(&event)
                {
                    *last = event;
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
