//! Event queue for window-level events.

use aurea_foundation::{EventCallback, WindowEvent};
use std::sync::Mutex;

pub struct EventQueue {
    events: Mutex<Vec<WindowEvent>>,
    callbacks: Mutex<Vec<EventCallback>>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            callbacks: Mutex::new(Vec::new()),
        }
    }

    pub fn push(&self, event: WindowEvent) {
        let mut events = aurea_foundation::lock(&self.events);
        // Coalesce high-frequency motion events: replace the tail if it is the
        // same variant, so a fast mouse or trackpad never queues more than one
        // entry per process_events() call.
        match &event {
            WindowEvent::MouseMove { .. }
            | WindowEvent::RawMouseMotion { .. }
            | WindowEvent::MouseWheel { .. } => {
                if let Some(last) = events.last_mut() {
                    if std::mem::discriminant(last) == std::mem::discriminant(&event) {
                        *last = event;
                        return;
                    }
                }
            }
            _ => {}
        }
        events.push(event);
    }

    pub fn pop_all(&self) -> Vec<WindowEvent> {
        let mut events = aurea_foundation::lock(&self.events);
        std::mem::take(&mut *events)
    }

    pub fn register_callback(&self, callback: EventCallback) {
        let mut callbacks = aurea_foundation::lock(&self.callbacks);
        callbacks.push(callback);
    }

    pub fn process_events(&self) -> Vec<WindowEvent> {
        let events = self.pop_all();
        if events.is_empty() {
            return Vec::new();
        }

        let callbacks: Vec<EventCallback> = {
            let callbacks = aurea_foundation::lock(&self.callbacks);
            callbacks.clone()
        };

        for event in &events {
            for callback in &callbacks {
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
