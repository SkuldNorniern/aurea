//! Per-window event queues and callbacks.
//!
//! The split here is deliberate. Event *queues* are global and thread-safe:
//! the platform can report an event from a thread that is not the UI thread,
//! so pushing must work from anywhere. Event *callbacks* are thread-local: they
//! run on the UI thread and may capture windows and widgets, which belong to
//! it.

use super::handle_key;
use crate::window::{WindowEvent, WindowId};
use aurea_foundation::{EventCallback, lock};
use aurea_runtime::EventQueue;
use std::{
    cell::RefCell,
    collections::HashMap,
    os::raw::c_void,
    rc::Rc,
    sync::{Arc, LazyLock, Mutex, Weak},
};

type WindowUpdateCallback = Rc<dyn Fn(WindowId)>;

static WINDOW_QUEUE_BY_HANDLE: LazyLock<Mutex<HashMap<usize, Weak<EventQueue>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

thread_local! {
    /// Event handlers registered through `Window::on_event`, by window handle.
    static WINDOW_EVENT_CALLBACKS: RefCell<HashMap<usize, Vec<EventCallback>>> =
        RefCell::new(HashMap::new());
    /// Per-frame update callbacks, by window handle.
    static WINDOW_UPDATE_CALLBACKS: RefCell<HashMap<usize, Vec<WindowUpdateCallback>>> =
        RefCell::new(HashMap::new());
}

pub fn register_event_queue(handle: *mut c_void, queue: &Arc<EventQueue>) {
    let mut by_handle = lock(&WINDOW_QUEUE_BY_HANDLE);
    by_handle.insert(handle_key(handle), Arc::downgrade(queue));
}

pub fn unregister_event_queue(handle: *mut c_void) {
    let mut by_handle = lock(&WINDOW_QUEUE_BY_HANDLE);
    by_handle.remove(&handle_key(handle));
}

pub fn register_update_callbacks(handle: *mut c_void) {
    WINDOW_UPDATE_CALLBACKS.with(|registry| {
        registry.borrow_mut().insert(handle_key(handle), Vec::new());
    });
}

pub fn unregister_update_callbacks(handle: *mut c_void) {
    WINDOW_UPDATE_CALLBACKS.with(|registry| {
        registry.borrow_mut().remove(&handle_key(handle));
    });
}

pub fn register_update_callback(handle: *mut c_void, callback: impl Fn(WindowId) + 'static) {
    WINDOW_UPDATE_CALLBACKS.with(|registry| {
        if let Some(list) = registry.borrow_mut().get_mut(&handle_key(handle)) {
            list.push(Rc::new(callback));
        }
    });
}

/// Registers an event handler for a window.
pub fn register_event_callback(handle: *mut c_void, callback: EventCallback) {
    WINDOW_EVENT_CALLBACKS.with(|registry| {
        registry
            .borrow_mut()
            .entry(handle_key(handle))
            .or_default()
            .push(callback);
    });
}

/// Drops every handler registered for a window.
pub fn unregister_event_callbacks(handle: *mut c_void) {
    WINDOW_EVENT_CALLBACKS.with(|registry| {
        registry.borrow_mut().remove(&handle_key(handle));
    });
}

/// Hands `events` to the window's registered handlers.
///
/// The handler list is cloned out before any handler runs, so a handler is free
/// to register another one without upsetting the borrow.
pub fn dispatch_window_events(handle: *mut c_void, events: &[WindowEvent]) {
    if events.is_empty() {
        return;
    }
    let callbacks = WINDOW_EVENT_CALLBACKS.with(|registry| {
        registry
            .borrow()
            .get(&handle_key(handle))
            .cloned()
            .unwrap_or_default()
    });

    for event in events {
        for callback in &callbacks {
            callback(event.clone());
        }
    }
}

pub fn push_window_event(handle: *mut c_void, event: WindowEvent) {
    let queue = {
        let mut by_handle = lock(&WINDOW_QUEUE_BY_HANDLE);
        match by_handle
            .get(&handle_key(handle))
            .and_then(|weak| weak.upgrade())
        {
            Some(q) => Some(q),
            None => {
                by_handle.remove(&handle_key(handle));
                None
            }
        }
    };

    if let Some(queue) = queue {
        queue.push(event);
    }
}

pub fn process_all_window_events() {
    let live: Vec<(usize, Arc<EventQueue>)> = {
        let mut by_handle = lock(&WINDOW_QUEUE_BY_HANDLE);
        by_handle.retain(|_, weak| weak.strong_count() > 0);
        by_handle
            .iter()
            .filter_map(|(handle, weak)| weak.upgrade().map(|queue| (*handle, queue)))
            .collect()
    };

    // The lock is released before dispatching: a handler may create or destroy
    // a window, which would otherwise re-enter it.
    for (handle, queue) in live {
        let events = queue.pop_all();
        dispatch_window_events(handle as *mut c_void, &events);
    }
}

pub fn process_all_window_updates() {
    let callbacks = WINDOW_UPDATE_CALLBACKS.with(|registry| {
        registry
            .borrow()
            .iter()
            .map(|(handle, list)| (WindowId::from_raw(*handle), list.clone()))
            .collect::<Vec<_>>()
    });

    for (window_id, list) in callbacks {
        for callback in list {
            callback(window_id);
        }
    }
}

pub fn process_window_updates(handle: *mut c_void) {
    let callbacks = WINDOW_UPDATE_CALLBACKS.with(|registry| {
        registry
            .borrow()
            .get(&handle_key(handle))
            .cloned()
            .unwrap_or_default()
    });

    let window_id = WindowId::from_handle(handle);
    for callback in callbacks {
        callback(window_id);
    }
}
