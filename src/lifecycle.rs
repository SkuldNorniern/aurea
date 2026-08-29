/// Lifecycle event types for application and window lifecycle management.
///
/// This module provides the infrastructure for handling lifecycle events
/// across desktop and mobile platforms, enabling proper handling of:
/// - Application lifecycle (background/foreground, suspend/resume)
/// - Window lifecycle (close, minimize, restore)
/// - Memory warnings
/// - Surface recreation (for mobile)
use aurea_foundation::lock;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::{Arc, LazyLock, Mutex};

/// Lifecycle event types that can be triggered by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleEvent {
    /// Application entered background (iOS: `applicationDidEnterBackground`)
    ApplicationDidEnterBackground,
    /// Application will enter foreground (iOS: `applicationWillEnterForeground`)
    ApplicationWillEnterForeground,
    /// Application paused (Android: `onPause`)
    ApplicationPaused,
    /// Application resumed (Android: `onResume`)
    ApplicationResumed,
    /// Application destroyed (Android: `onDestroy`)
    ApplicationDestroyed,
    /// Window will close
    WindowWillClose,
    /// Window minimized
    WindowMinimized,
    /// Window restored from minimized state
    WindowRestored,
    /// Window moved
    WindowMoved,
    /// Window resized
    WindowResized,
    /// Memory warning received (iOS: `didReceiveMemoryWarning`)
    MemoryWarning,
    /// Surface lost (mobile: OpenGL/Vulkan context lost)
    SurfaceLost,
    /// Surface recreated (mobile: OpenGL/Vulkan context recreated)
    SurfaceRecreated,
}

/// Callback function type for lifecycle events.
///
/// `Arc` so a dispatch can clone it out and drop the registry lock before
/// running it. Window teardown registers and unregisters lifecycle callbacks,
/// and a callback that closes a window would otherwise deadlock on the lock
/// that is dispatching it.
pub type LifecycleCallback = Arc<dyn Fn(LifecycleEvent) + Send + Sync>;

/// What is listening to one window's lifecycle events.
///
/// Aurea's own bridge is kept apart from anything the application registers.
/// They shared a slot before, so subscribing replaced the bridge and the
/// window quietly stopped delivering `CloseRequested`, `Resized` and `Moved`
/// to its event queue.
#[derive(Default)]
struct WindowLifecycle {
    /// The bridge `Window::new` installs, which fills the event queue.
    internal: Option<LifecycleCallback>,
    /// Whatever the application asked to hear about, in the order it asked.
    subscribers: Vec<LifecycleCallback>,
}

impl WindowLifecycle {
    /// Everything to call for one event: the bridge first, so the queue is
    /// filled before an application callback can look at it.
    fn listeners(&self) -> Vec<LifecycleCallback> {
        self.internal
            .iter()
            .chain(self.subscribers.iter())
            .cloned()
            .collect()
    }
}

/// Global registry for lifecycle callbacks per window.
///
/// This allows multiple windows to register their own lifecycle callbacks.
/// We use a raw pointer as the key, which is safe because we only use it for
/// comparison and the window handle is stable for the lifetime of the window.
static LIFECYCLE_CALLBACKS: LazyLock<Mutex<HashMap<usize, WindowLifecycle>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Registers Aurea's own bridge for a window, replacing any previous one.
///
/// For the framework's use. An application wants
/// [`subscribe_lifecycle_callback`], which adds a listener rather than taking
/// this one's place.
pub fn register_lifecycle_callback(window: *mut c_void, callback: LifecycleCallback) {
    let mut callbacks = lock(&LIFECYCLE_CALLBACKS);
    callbacks.entry(window as usize).or_default().internal = Some(callback);
}

/// Adds an application listener for a window's lifecycle events.
///
/// Every listener is called, in the order they were added, after Aurea's own
/// bridge has run.
pub fn subscribe_lifecycle_callback(window: *mut c_void, callback: LifecycleCallback) {
    let mut callbacks = lock(&LIFECYCLE_CALLBACKS);
    callbacks
        .entry(window as usize)
        .or_default()
        .subscribers
        .push(callback);
}

/// Unregister the lifecycle callback for a specific window.
pub fn unregister_lifecycle_callback(window: *mut c_void) {
    let mut callbacks = lock(&LIFECYCLE_CALLBACKS);
    callbacks.remove(&(window as usize));
}

/// Invoke the lifecycle callback for a specific window.
///
/// This is called from the FFI layer when a lifecycle event occurs.
pub fn invoke_lifecycle_callback(window: *mut c_void, event: LifecycleEvent) {
    // Collected and the lock released first: a listener may create or drop a
    // window, which would come back through here.
    let listeners = lock(&LIFECYCLE_CALLBACKS)
        .get(&(window as usize))
        .map(WindowLifecycle::listeners)
        .unwrap_or_default();
    for listener in listeners {
        listener(event);
    }
}

/// Invoke a global lifecycle callback (not tied to a specific window).
///
/// This is used for application-level events that affect the entire app.
pub fn invoke_global_lifecycle_callback(event: LifecycleEvent) {
    let listeners: Vec<LifecycleCallback> = lock(&LIFECYCLE_CALLBACKS)
        .values()
        .flat_map(WindowLifecycle::listeners)
        .collect();
    for listener in listeners {
        listener(event);
    }
}

/// Maps platform event IDs (e.g. from Android JNI, iOS) to LifecycleEvent.
/// Used by ng_invoke_lifecycle_callback and for lifecycle mapping tests.
pub fn event_from_id(event_id: u32) -> Option<LifecycleEvent> {
    let event = match event_id {
        0 => LifecycleEvent::ApplicationDidEnterBackground,
        1 => LifecycleEvent::ApplicationWillEnterForeground,
        2 => LifecycleEvent::ApplicationPaused,
        3 => LifecycleEvent::ApplicationResumed,
        4 => LifecycleEvent::ApplicationDestroyed,
        5 => LifecycleEvent::WindowWillClose,
        6 => LifecycleEvent::WindowMinimized,
        7 => LifecycleEvent::WindowRestored,
        8 => LifecycleEvent::MemoryWarning,
        9 => LifecycleEvent::SurfaceLost,
        10 => LifecycleEvent::SurfaceRecreated,
        11 => LifecycleEvent::WindowMoved,
        12 => LifecycleEvent::WindowResized,
        _ => return None,
    };
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Subscribing used to take the framework's slot, so a window that added
    /// a listener stopped delivering its own events.
    #[test]
    fn subscribing_does_not_displace_the_internal_bridge() {
        let window = 0x1001 as *mut c_void;
        let bridge = Arc::new(AtomicU32::new(0));
        let listener = Arc::new(AtomicU32::new(0));

        let b = Arc::clone(&bridge);
        register_lifecycle_callback(
            window,
            Arc::new(move |_| {
                b.fetch_add(1, Ordering::Relaxed);
            }),
        );
        let l = Arc::clone(&listener);
        subscribe_lifecycle_callback(
            window,
            Arc::new(move |_| {
                l.fetch_add(1, Ordering::Relaxed);
            }),
        );

        invoke_lifecycle_callback(window, LifecycleEvent::WindowWillClose);

        assert_eq!(bridge.load(Ordering::Relaxed), 1, "the bridge still runs");
        assert_eq!(
            listener.load(Ordering::Relaxed),
            1,
            "and so does the listener"
        );
        unregister_lifecycle_callback(window);
    }

    /// Several listeners all hear about it, in the order they were added.
    #[test]
    fn every_subscriber_is_called_in_order() {
        let window = 0x1002 as *mut c_void;
        let order = Arc::new(Mutex::new(Vec::new()));

        for tag in 1..=3u32 {
            let seen = Arc::clone(&order);
            subscribe_lifecycle_callback(
                window,
                Arc::new(move |_| {
                    lock(&seen).push(tag);
                }),
            );
        }

        invoke_lifecycle_callback(window, LifecycleEvent::WindowRestored);

        assert_eq!(*lock(&order), vec![1, 2, 3]);
        unregister_lifecycle_callback(window);
    }

    /// Dropping a window forgets its listeners along with its bridge.
    #[test]
    fn unregistering_clears_every_listener() {
        let window = 0x1003 as *mut c_void;
        let calls = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&calls);
        subscribe_lifecycle_callback(
            window,
            Arc::new(move |_| {
                c.fetch_add(1, Ordering::Relaxed);
            }),
        );

        unregister_lifecycle_callback(window);
        invoke_lifecycle_callback(window, LifecycleEvent::WindowRestored);

        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn lifecycle_event_ids_map_to_events() {
        assert_eq!(
            event_from_id(0),
            Some(LifecycleEvent::ApplicationDidEnterBackground)
        );
        assert_eq!(
            event_from_id(1),
            Some(LifecycleEvent::ApplicationWillEnterForeground)
        );
        assert_eq!(event_from_id(2), Some(LifecycleEvent::ApplicationPaused));
        assert_eq!(event_from_id(3), Some(LifecycleEvent::ApplicationResumed));
        assert_eq!(event_from_id(4), Some(LifecycleEvent::ApplicationDestroyed));
        assert_eq!(event_from_id(9), Some(LifecycleEvent::SurfaceLost));
        assert_eq!(event_from_id(10), Some(LifecycleEvent::SurfaceRecreated));
        assert_eq!(event_from_id(99), None);
    }

    #[test]
    fn lifecycle_callback_invoked_on_pause_resume_surface_lost() {
        let received = Arc::new(AtomicU32::new(0));
        let r = received.clone();
        register_lifecycle_callback(
            0x1000 as *mut c_void,
            Arc::new(move |e| {
                let id = match e {
                    LifecycleEvent::ApplicationPaused => 2,
                    LifecycleEvent::ApplicationResumed => 3,
                    LifecycleEvent::SurfaceLost => 9,
                    LifecycleEvent::SurfaceRecreated => 10,
                    _ => 0,
                };
                r.store(id, Ordering::SeqCst);
            }),
        );

        invoke_lifecycle_callback(0x1000 as *mut c_void, LifecycleEvent::ApplicationPaused);
        assert_eq!(received.load(Ordering::SeqCst), 2);

        invoke_lifecycle_callback(0x1000 as *mut c_void, LifecycleEvent::ApplicationResumed);
        assert_eq!(received.load(Ordering::SeqCst), 3);

        invoke_lifecycle_callback(0x1000 as *mut c_void, LifecycleEvent::SurfaceLost);
        assert_eq!(received.load(Ordering::SeqCst), 9);

        invoke_lifecycle_callback(0x1000 as *mut c_void, LifecycleEvent::SurfaceRecreated);
        assert_eq!(received.load(Ordering::SeqCst), 10);

        unregister_lifecycle_callback(0x1000 as *mut c_void);
        invoke_lifecycle_callback(0x1000 as *mut c_void, LifecycleEvent::ApplicationPaused);
        assert_eq!(received.load(Ordering::SeqCst), 10);
    }
}
