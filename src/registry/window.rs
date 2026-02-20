use crate::runtime::event_queue::EventQueue;
use crate::window::{WindowEvent, WindowId};
use std::{
    collections::HashMap,
    os::raw::c_void,
    sync::{Arc, LazyLock, Mutex, Weak},
};

type WindowUpdateCallback = Arc<dyn Fn(WindowId) + Send + Sync>;

static WINDOW_EVENT_QUEUES: LazyLock<Mutex<Vec<Weak<EventQueue>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static WINDOW_QUEUE_BY_HANDLE: LazyLock<Mutex<HashMap<usize, Weak<EventQueue>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static WINDOW_UPDATE_CALLBACKS: LazyLock<
    Mutex<HashMap<usize, Arc<Mutex<Vec<WindowUpdateCallback>>>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn register_global_event_queue(queue: &Arc<EventQueue>) {
    let mut queues = crate::sync::lock(&WINDOW_EVENT_QUEUES);
    queues.push(Arc::downgrade(queue));
}

pub fn register_event_queue(handle: *mut c_void, queue: &Arc<EventQueue>) {
    let mut by_handle = crate::sync::lock(&WINDOW_QUEUE_BY_HANDLE);
    by_handle.insert(handle as usize, Arc::downgrade(queue));
}

pub fn unregister_event_queue(handle: *mut c_void) {
    let mut by_handle = crate::sync::lock(&WINDOW_QUEUE_BY_HANDLE);
    by_handle.remove(&(handle as usize));
}

pub fn register_update_callbacks(handle: *mut c_void) {
    let mut callbacks = crate::sync::lock(&WINDOW_UPDATE_CALLBACKS);
    callbacks.insert(handle as usize, Arc::new(Mutex::new(Vec::new())));
}

pub fn unregister_update_callbacks(handle: *mut c_void) {
    let mut callbacks = crate::sync::lock(&WINDOW_UPDATE_CALLBACKS);
    callbacks.remove(&(handle as usize));
}

pub fn register_update_callback(
    handle: *mut c_void,
    callback: impl Fn(WindowId) + Send + Sync + 'static,
) {
    let callbacks = {
        let callbacks = crate::sync::lock(&WINDOW_UPDATE_CALLBACKS);
        callbacks.get(&(handle as usize)).cloned()
    };

    if let Some(list) = callbacks {
        let mut guard = crate::sync::lock(list.as_ref());
        guard.push(Arc::new(callback));
    }
}

pub fn push_window_event(handle: *mut c_void, event: WindowEvent) {
    let queue = {
        let mut by_handle = crate::sync::lock(&WINDOW_QUEUE_BY_HANDLE);
        match by_handle
            .get(&(handle as usize))
            .and_then(|weak| weak.upgrade())
        {
            Some(q) => Some(q),
            None => {
                by_handle.remove(&(handle as usize));
                None
            }
        }
    };

    if let Some(queue) = queue {
        queue.push(event);
    }
}

pub fn process_all_window_events() {
    let mut queues = crate::sync::lock(&WINDOW_EVENT_QUEUES);
    queues.retain(|weak| {
        if let Some(queue) = weak.upgrade() {
            queue.process_events();
            true
        } else {
            false
        }
    });
}

pub fn process_all_window_updates() {
    let callbacks = {
        let registry = crate::sync::lock(&WINDOW_UPDATE_CALLBACKS);
        registry
            .iter()
            .map(|(handle, list)| {
                let list = crate::sync::lock(list.as_ref()).clone();
                (WindowId::from_raw(*handle), list)
            })
            .collect::<Vec<_>>()
    };

    for (window_id, list) in callbacks {
        for callback in list {
            callback(window_id);
        }
    }
}

pub fn process_window_updates(handle: *mut c_void) {
    let callbacks = {
        let registry = crate::sync::lock(&WINDOW_UPDATE_CALLBACKS);
        registry.get(&(handle as usize)).cloned()
    }
    .map(|list| crate::sync::lock(list.as_ref()).clone())
    .unwrap_or_default();

    let window_id = WindowId::from_handle(handle);
    for callback in callbacks {
        callback(window_id);
    }
}
