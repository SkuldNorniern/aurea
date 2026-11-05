//! Per-window event queues and callbacks.

use super::handle_key;
use crate::window::{WindowEvent, WindowId};
use aurea_foundation::{EventCallback, lock};
use aurea_runtime::{EventQueue, FrameScheduler};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    os::raw::c_void,
    rc::Rc,
    sync::{Arc, LazyLock, Mutex, Weak},
};

type WindowUpdateCallback = Rc<dyn Fn(WindowId)>;

static WINDOW_QUEUE_BY_HANDLE: LazyLock<Mutex<HashMap<usize, Weak<EventQueue>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static SCALE_BY_HANDLE: LazyLock<Mutex<HashMap<usize, f32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

thread_local! {
    /// Shared so a dispatch can hold a snapshot without copying the list.
    static WINDOW_EVENT_CALLBACKS: RefCell<HashMap<usize, Rc<Vec<EventCallback>>>> =
        RefCell::new(HashMap::new());
    static WINDOW_UPDATE_CALLBACKS: RefCell<HashMap<usize, Vec<WindowUpdateCallback>>> =
        RefCell::new(HashMap::new());
    static READER_PUMPS: Cell<u32> = const { Cell::new(0) };
    /// Events a frame dispatched during a reader's pump, waiting for that window's next read.
    static UNREAD: RefCell<HashMap<usize, Vec<WindowEvent>>> = RefCell::new(HashMap::new());
}

/// Most events kept for a window no one reads.
const MAX_UNREAD: usize = 4096;

/// Runs `pump` for a loop that reads events back, which a frame firing inside must not take away.
pub(crate) fn pump_for_reader(pump: impl FnOnce()) {
    READER_PUMPS.with(|pumps| pumps.set(pumps.get() + 1));
    pump();
    READER_PUMPS.with(|pumps| pumps.set(pumps.get() - 1));
}

/// Takes what frames dispatched for the window since its last read.
pub(crate) fn take_unread(handle: *mut c_void) -> Vec<WindowEvent> {
    UNREAD
        .with(|unread| unread.borrow_mut().remove(&handle_key(handle)))
        .unwrap_or_default()
}

/// Associates `queue` with the window handle.
pub fn register_event_queue(handle: *mut c_void, queue: &Arc<EventQueue>) {
    let mut by_handle = lock(&WINDOW_QUEUE_BY_HANDLE);
    by_handle.insert(handle_key(handle), Arc::downgrade(queue));
}

/// Forgets the event queue and scale of the window handle.
pub fn unregister_event_queue(handle: *mut c_void) {
    let mut by_handle = lock(&WINDOW_QUEUE_BY_HANDLE);
    by_handle.remove(&handle_key(handle));
    lock(&SCALE_BY_HANDLE).remove(&handle_key(handle));
    take_unread(handle);
}

/// Records `scale` for the window, returning whether it differs from the last one recorded.
pub fn scale_factor_changed(handle: *mut c_void, scale: f32) -> bool {
    let previous = lock(&SCALE_BY_HANDLE).insert(handle_key(handle), scale);
    previous.is_none_or(|old| (old - scale).abs() > f32::EPSILON)
}

/// Creates an empty update callback list for the window.
pub fn register_update_callbacks(handle: *mut c_void) {
    WINDOW_UPDATE_CALLBACKS.with(|registry| {
        registry.borrow_mut().insert(handle_key(handle), Vec::new());
    });
}

/// Drops the update callbacks for the window.
pub fn unregister_update_callbacks(handle: *mut c_void) {
    WINDOW_UPDATE_CALLBACKS.with(|registry| {
        registry.borrow_mut().remove(&handle_key(handle));
    });
}

/// Adds `callback` to the window's update callbacks.
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
        // Copy-on-write: a dispatch in flight keeps its snapshot and does not see this callback.
        let mut registry = registry.borrow_mut();
        Rc::make_mut(registry.entry(handle_key(handle)).or_default()).push(callback);
    });
}

/// Drops every handler registered for a window.
pub fn unregister_event_callbacks(handle: *mut c_void) {
    WINDOW_EVENT_CALLBACKS.with(|registry| {
        registry.borrow_mut().remove(&handle_key(handle));
    });
}

/// Hands `events` to the window's registered handlers.
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
        for callback in callbacks.iter() {
            callback(event);
        }
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    #[test]
    fn a_frame_during_a_readers_pump_leaves_the_events_for_the_read() {
        let window = 0xd15c as *mut c_void;
        let queue = EventQueue::new();
        let heard = Rc::new(Cell::new(0));
        let counter = heard.clone();
        register_event_callback(
            window,
            Rc::new(move |_: &WindowEvent| counter.set(counter.get() + 1)),
        );

        queue.push(WindowEvent::Focused);
        pump_for_reader(|| dispatch_queue(window, &queue));
        assert_eq!(heard.get(), 1, "callbacks hear it once, from the frame");
        assert!(matches!(take_unread(window)[..], [WindowEvent::Focused]));
        assert!(take_unread(window).is_empty());

        queue.push(WindowEvent::Focused);
        dispatch_queue(window, &queue);
        assert!(
            take_unread(window).is_empty(),
            "a native run loop reads nothing"
        );
        unregister_event_callbacks(window);
    }

    #[test]
    fn a_window_no_one_reads_keeps_only_its_newest_events() {
        let window = 0xd15d as *mut c_void;
        let queue = EventQueue::new();
        pump_for_reader(|| {
            for _ in 0..=MAX_UNREAD {
                queue.push(WindowEvent::Focused);
                queue.push(WindowEvent::Unfocused);
                dispatch_queue(window, &queue);
            }
        });
        let unread = take_unread(window);
        assert_eq!(unread.len(), MAX_UNREAD);
        assert!(matches!(unread.last(), Some(WindowEvent::Unfocused)));
    }

    #[test]
    fn every_listener_gets_each_event_in_the_order_added() {
        let window = 0xd15b as *mut c_void;
        let log = Rc::new(RefCell::new(Vec::<&str>::new()));
        for name in ["first", "second"] {
            let log = log.clone();
            register_event_callback(
                window,
                Rc::new(move |_: &WindowEvent| log.borrow_mut().push(name)),
            );
        }

        dispatch_window_events(window, &[WindowEvent::Focused]);
        assert_eq!(*log.borrow(), ["first", "second"]);
        unregister_event_callbacks(window);
    }

    #[test]
    fn callbacks_added_during_dispatch_run_next_time() {
        let window = 0xd15a as *mut c_void;
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let added = Rc::new(Cell::new(false));

        let first_log = log.clone();
        let first_added = added.clone();
        register_event_callback(
            window,
            Rc::new(move |e: &WindowEvent| {
                first_log.borrow_mut().push(format!("a {e:?}"));
                if !first_added.replace(true) {
                    let late_log = first_log.clone();
                    register_event_callback(
                        window,
                        Rc::new(move |e: &WindowEvent| {
                            late_log.borrow_mut().push(format!("late {e:?}"));
                        }),
                    );
                }
            }),
        );
        let second_log = log.clone();
        register_event_callback(
            window,
            Rc::new(move |e: &WindowEvent| second_log.borrow_mut().push(format!("b {e:?}"))),
        );

        let events = [
            WindowEvent::ThemeChanged { dark: true },
            WindowEvent::ThemeChanged { dark: false },
        ];
        dispatch_window_events(window, &events);
        let first_pass = log.borrow().clone();
        assert_eq!(
            first_pass,
            [
                "a ThemeChanged { dark: true }",
                "b ThemeChanged { dark: true }",
                "a ThemeChanged { dark: false }",
                "b ThemeChanged { dark: false }",
            ]
        );

        log.borrow_mut().clear();
        dispatch_window_events(window, &events[..1]);
        assert_eq!(
            *log.borrow(),
            [
                "a ThemeChanged { dark: true }",
                "b ThemeChanged { dark: true }",
                "late ThemeChanged { dark: true }",
            ]
        );
        unregister_event_callbacks(window);
    }
}

/// Queues `event` for the window and wakes the frame loop.
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
        FrameScheduler::wake();
    }
}

/// Queues `event` for every live window, for a change that belongs to none in particular.
pub fn push_event_to_all_windows(event: WindowEvent) {
    let queues: Vec<Arc<EventQueue>> = {
        let by_handle = lock(&WINDOW_QUEUE_BY_HANDLE);
        by_handle.values().filter_map(Weak::upgrade).collect()
    };
    for queue in &queues {
        queue.push(event.clone());
    }
    if !queues.is_empty() {
        FrameScheduler::wake();
    }
}

/// Dispatches queued events for every live window.
pub fn process_all_window_events() {
    let live: Vec<(usize, Arc<EventQueue>)> = {
        let mut by_handle = lock(&WINDOW_QUEUE_BY_HANDLE);
        by_handle.retain(|_, weak| weak.strong_count() > 0);
        by_handle
            .iter()
            .filter_map(|(handle, weak)| weak.upgrade().map(|queue| (*handle, queue)))
            .collect()
    };

    for (handle, queue) in live {
        dispatch_queue(handle as *mut c_void, &queue);
    }
}

fn dispatch_queue(handle: *mut c_void, queue: &EventQueue) {
    let events = queue.pop_all();
    dispatch_window_events(handle, &events);
    if events.is_empty() || READER_PUMPS.with(Cell::get) == 0 {
        return;
    }
    UNREAD.with(|unread| {
        let mut unread = unread.borrow_mut();
        let kept = unread.entry(handle_key(handle)).or_default();
        kept.extend(events);
        kept.drain(..kept.len().saturating_sub(MAX_UNREAD));
    });
}

/// Runs update callbacks for every window.
pub fn process_all_window_updates() {
    let callbacks = WINDOW_UPDATE_CALLBACKS.with(|registry| {
        registry
            .borrow()
            .iter()
            .filter_map(|(handle, list)| Some((WindowId::of_key(*handle)?, list.clone())))
            .collect::<Vec<_>>()
    });

    for (window_id, list) in callbacks {
        for callback in list {
            callback(window_id);
        }
    }
}

/// Runs the update callbacks of one window.
pub fn process_window_updates(handle: *mut c_void) {
    let callbacks = WINDOW_UPDATE_CALLBACKS.with(|registry| {
        registry
            .borrow()
            .get(&handle_key(handle))
            .cloned()
            .unwrap_or_default()
    });

    let Some(window_id) = WindowId::of_handle(handle) else {
        return;
    };
    for callback in callbacks {
        callback(window_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_new_scale_counts_as_a_change() {
        let window = 0x5ca1e as *mut c_void;
        assert!(scale_factor_changed(window, 2.0));
        assert!(!scale_factor_changed(window, 2.0));
        assert!(scale_factor_changed(window, 1.5));
        unregister_event_queue(window);
        assert!(
            scale_factor_changed(window, 1.5),
            "a new window starts fresh"
        );
        unregister_event_queue(window);
    }

    #[test]
    fn a_theme_change_reaches_every_window() {
        let (a, b) = (0x7e1a as *mut c_void, 0x7e1b as *mut c_void);
        let (qa, qb) = (Arc::new(EventQueue::new()), Arc::new(EventQueue::new()));
        register_event_queue(a, &qa);
        register_event_queue(b, &qb);
        push_event_to_all_windows(WindowEvent::ThemeChanged { dark: true });
        for queue in [&qa, &qb] {
            let events = queue.pop_all();
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, WindowEvent::ThemeChanged { dark: true })),
                "{events:?}"
            );
        }
        unregister_event_queue(a);
        unregister_event_queue(b);
    }
}
