//! Shared ID allocation and callback storage for the platform-callback registries.

use aurea_foundation::lock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// Monotonic `u32` id allocator, starting at 1.
pub(crate) struct IdAllocator(AtomicU32);

impl IdAllocator {
    pub const fn new() -> Self {
        Self(AtomicU32::new(1))
    }

    pub fn next(&self) -> u32 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

/// A callback registry keyed by `u32` id, shared by every widget kind that
/// registers a platform callback (button clicks, text-changed, tab selection, …).
///
/// `invoke` clones the callback out from under the lock before calling it, so a
/// callback that registers another callback of the same kind cannot deadlock.
pub(crate) struct CallbackRegistry<A: 'static> {
    slots: LazyLock<Mutex<HashMap<u32, Arc<dyn Fn(A) + Send + Sync>>>>,
}

impl<A> CallbackRegistry<A> {
    pub const fn new() -> Self {
        Self {
            slots: LazyLock::new(|| Mutex::new(HashMap::new())),
        }
    }

    pub fn insert(&self, id: u32, callback: impl Fn(A) + Send + Sync + 'static) {
        lock(&self.slots).insert(id, Arc::new(callback));
    }

    pub fn invoke(&self, id: u32, arg: A) {
        let callback = lock(&self.slots).get(&id).cloned();
        if let Some(callback) = callback {
            callback(arg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_allocator_increments_from_one() {
        let ids = IdAllocator::new();
        assert_eq!(ids.next(), 1);
        assert_eq!(ids.next(), 2);
        assert_eq!(ids.next(), 3);
    }

    #[test]
    fn callback_registry_invokes_registered_callback() {
        static REGISTRY: CallbackRegistry<i32> = CallbackRegistry::new();
        let seen = Arc::new(Mutex::new(0));
        let seen_clone = Arc::clone(&seen);
        REGISTRY.insert(1, move |value| *lock(&seen_clone) = value);
        REGISTRY.invoke(1, 42);
        assert_eq!(*lock(&seen), 42);
    }

    #[test]
    fn callback_registry_invoke_on_unknown_id_is_noop() {
        static REGISTRY: CallbackRegistry<i32> = CallbackRegistry::new();
        REGISTRY.invoke(999, 0);
    }

    /// A callback that re-registers a callback for the same id while it runs
    /// must not deadlock: `invoke` clones the `Arc` out before calling.
    #[test]
    fn callback_registry_invoke_allows_reentrant_insert() {
        static REGISTRY: CallbackRegistry<i32> = CallbackRegistry::new();
        REGISTRY.insert(1, |value| {
            if value < 3 {
                REGISTRY.insert(1, |_| {});
                REGISTRY.invoke(1, value + 1);
            }
        });
        REGISTRY.invoke(1, 0);
    }
}
