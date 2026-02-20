/// Lifecycle event types for application and window lifecycle management.
///
/// This module provides the infrastructure for handling lifecycle events
/// across desktop and mobile platforms, enabling proper handling of:
/// - Application lifecycle (background/foreground, suspend/resume)
/// - Window lifecycle (close, minimize, restore)
/// - Memory warnings
/// - Surface recreation (for mobile)
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::{LazyLock, Mutex};

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
pub type LifecycleCallback = Box<dyn Fn(LifecycleEvent) + Send + Sync>;

/// Global registry for lifecycle callbacks per window.
///
/// This allows multiple windows to register their own lifecycle callbacks.
/// We use a raw pointer as the key, which is safe because we only use it for
/// comparison and the window handle is stable for the lifetime of the window.
static LIFECYCLE_CALLBACKS: LazyLock<Mutex<HashMap<usize, LifecycleCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a lifecycle callback for a specific window.
///
/// The callback will be invoked when lifecycle events occur for the given window.
/// Only one callback can be registered per window; registering a new callback
/// replaces any existing one.
pub fn register_lifecycle_callback(window: *mut c_void, callback: LifecycleCallback) {
    let mut callbacks = crate::sync::lock(&LIFECYCLE_CALLBACKS);
    callbacks.insert(window as usize, callback);
}

/// Unregister the lifecycle callback for a specific window.
pub fn unregister_lifecycle_callback(window: *mut c_void) {
    let mut callbacks = crate::sync::lock(&LIFECYCLE_CALLBACKS);
    callbacks.remove(&(window as usize));
}

/// Invoke the lifecycle callback for a specific window.
///
/// This is called from the FFI layer when a lifecycle event occurs.
pub fn invoke_lifecycle_callback(window: *mut c_void, event: LifecycleEvent) {
    let callbacks = crate::sync::lock(&LIFECYCLE_CALLBACKS);
    if let Some(callback) = callbacks.get(&(window as usize)) {
        callback(event);
    }
}

/// Invoke a global lifecycle callback (not tied to a specific window).
///
/// This is used for application-level events that affect the entire app.
pub fn invoke_global_lifecycle_callback(event: LifecycleEvent) {
    let callbacks = crate::sync::lock(&LIFECYCLE_CALLBACKS);
    for callback in callbacks.values() {
        callback(event);
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
    use std::sync::atomic::{AtomicU32, Ordering};

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
        let received = std::sync::Arc::new(AtomicU32::new(0));
        let r = received.clone();
        register_lifecycle_callback(0x1000 as *mut c_void, Box::new(move |e| {
            let id = match e {
                LifecycleEvent::ApplicationPaused => 2,
                LifecycleEvent::ApplicationResumed => 3,
                LifecycleEvent::SurfaceLost => 9,
                LifecycleEvent::SurfaceRecreated => 10,
                _ => 0,
            };
            r.store(id, Ordering::SeqCst);
        }));

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
