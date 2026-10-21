//! Custom callback registry for SwiftUI and other platform-triggered actions.

use super::callback::{CallbackRegistry, IdAllocator};

static CUSTOM_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static CUSTOM_CALLBACKS: CallbackRegistry<()> = CallbackRegistry::new();
}

/// Allocates a custom callback ID.
pub fn next_custom_id() -> u32 {
    CUSTOM_ID.next()
}

/// Registers `callback` for custom `id`.
pub fn register_custom_callback(id: u32, callback: impl Fn() + 'static) {
    CUSTOM_CALLBACKS.with(|r| r.insert(id, move |()| callback()));
}

/// Drops the custom callback registered for `id`.
pub fn unregister_custom_callback(id: u32) {
    CUSTOM_CALLBACKS.with(|r| r.remove(id));
}

/// Runs the callback registered for custom `id`, if any.
pub fn invoke_custom_callback(id: u32) {
    CUSTOM_CALLBACKS.with(|r| r.invoke(id, ()));
}
