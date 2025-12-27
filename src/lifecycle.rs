use std::os::raw::c_void;
use std::sync::{LazyLock, Mutex};
use std::collections::HashMap;

/// Lifecycle event types that can be triggered by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleEvent {
    /// Application entered background (iOS: `applicationDidEnterBackground`).
    ApplicationDidEnterBackground,
    /// Application will enter foreground (iOS: `applicationWillEnterForeground`).
    ApplicationWillEnterForeground,
    /// Application paused (Android: `onPause`).
    ApplicationPaused,
    /// Application resumed (Android: `onResume`).
    ApplicationResumed,
    /// Application destroyed (Android: `onDestroy`).
    ApplicationDestroyed,
    /// Window will close.
    WindowWillClose,
    /// Window minimized.
    WindowMinimized,
    /// Window restored from minimized state.
    WindowRestored,
    /// Memory warning received (iOS: `didReceiveMemoryWarning`).
    MemoryWarning,
    /// Surface lost (mobile: OpenGL/Vulkan context lost).
    SurfaceLost,
    /// Surface recreated (mobile: OpenGL/Vulkan context recreated).
    SurfaceRecreated,
}

/// Callback function type for lifecycle events.
pub type LifecycleCallback = Box<dyn Fn(LifecycleEvent) + Send + Sync>;

static LIFECYCLE_CALLBACKS: LazyLock<Mutex<HashMap<usize, LifecycleCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a lifecycle callback for a specific window.
pub fn register_lifecycle_callback(window: *mut c_void, callback: LifecycleCallback) {
    let mut callbacks = LIFECYCLE_CALLBACKS.lock().unwrap();
    callbacks.insert(window as usize, callback);
}

/// Unregister the lifecycle callback for a specific window.
pub fn unregister_lifecycle_callback(window: *mut c_void) {
    let mut callbacks = LIFECYCLE_CALLBACKS.lock().unwrap();
    callbacks.remove(&(window as usize));
}

/// Invoke the lifecycle callback for a specific window.
pub fn invoke_lifecycle_callback(window: *mut c_void, event: LifecycleEvent) {
    let callbacks = LIFECYCLE_CALLBACKS.lock().unwrap();
    if let Some(callback) = callbacks.get(&(window as usize)) {
        callback(event);
    }
}

/// Invoke a global lifecycle callback (not tied to a specific window).
pub fn invoke_global_lifecycle_callback(event: LifecycleEvent) {
    let callbacks = LIFECYCLE_CALLBACKS.lock().unwrap();
    for callback in callbacks.values() {
        callback(event);
    }
}

