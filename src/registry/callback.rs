//! Shared ID allocation and callback storage for the platform-callback registries.

use log::error;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

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
/// Lives in a `thread_local!`, not a `static`: platform callbacks are delivered
/// on the thread that owns the native UI, so that is the only thread that ever
/// touches one. Being thread-local is what lets the stored closures drop the
/// `Send + Sync` bound — an application can capture a `Window`, a widget, or
/// anything else that belongs to the UI thread.
///
/// `invoke` clones the callback out from under the borrow before calling it, so
/// a callback that registers another callback of the same kind cannot panic on
/// a re-entrant borrow.
/// A stored callback. `Rc`, not `Arc`: the registry never leaves its thread.
type Callback<A> = Rc<dyn Fn(A)>;

pub(crate) struct CallbackRegistry<A: 'static> {
    slots: RefCell<HashMap<u32, Callback<A>>>,
}

impl<A> CallbackRegistry<A> {
    pub fn new() -> Self {
        Self {
            slots: RefCell::new(HashMap::new()),
        }
    }

    pub fn insert(&self, id: u32, callback: impl Fn(A) + 'static) {
        match self.slots.try_borrow_mut() {
            Ok(mut slots) => {
                slots.insert(id, Rc::new(callback));
            }
            Err(_) => error!("aurea: callback registry busy; registration for id {id} dropped"),
        }
    }

    /// Drops the callback for `id`, if there is one.
    pub fn remove(&self, id: u32) {
        match self.slots.try_borrow_mut() {
            Ok(mut slots) => {
                slots.remove(&id);
            }
            Err(_) => error!("aurea: callback registry busy; removal for id {id} skipped"),
        }
    }

    /// How many callbacks are registered. Used by tests.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.try_borrow().map_or(0, |slots| slots.len())
    }

    pub fn invoke(&self, id: u32, arg: A) {
        // Clone out and release the borrow before calling: the callback is
        // free to register or invoke callbacks of the same kind.
        let callback = match self.slots.try_borrow() {
            Ok(slots) => slots.get(&id).map(Callback::clone),
            Err(_) => {
                error!("aurea: callback registry busy; invocation for id {id} skipped");
                None
            }
        };
        if let Some(callback) = callback {
            callback(arg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn id_allocator_increments_from_one() {
        let ids = IdAllocator::new();
        assert_eq!(ids.next(), 1);
        assert_eq!(ids.next(), 2);
        assert_eq!(ids.next(), 3);
    }

    #[test]
    fn callback_registry_invokes_registered_callback() {
        thread_local! {
            static REGISTRY: CallbackRegistry<i32> = CallbackRegistry::new();
        }
        let seen = Rc::new(Cell::new(0));
        let seen_clone = Rc::clone(&seen);
        REGISTRY.with(|r| r.insert(1, move |value| seen_clone.set(value)));
        REGISTRY.with(|r| r.invoke(1, 42));
        assert_eq!(seen.get(), 42);
    }

    #[test]
    fn callback_registry_invoke_on_unknown_id_is_noop() {
        thread_local! {
            static REGISTRY: CallbackRegistry<i32> = CallbackRegistry::new();
        }
        REGISTRY.with(|r| r.invoke(999, 0));
    }

    /// A callback that re-registers a callback for the same id while it runs
    /// must not fail: `invoke` clones the `Rc` out before calling.
    #[test]
    fn callback_registry_invoke_allows_reentrant_insert() {
        thread_local! {
            static REGISTRY: CallbackRegistry<i32> = CallbackRegistry::new();
        }
        let depth = Rc::new(Cell::new(0));
        let depth_clone = Rc::clone(&depth);
        REGISTRY.with(|r| {
            r.insert(1, move |value| {
                depth_clone.set(value);
                if value < 3 {
                    REGISTRY.with(|r| r.insert(1, |_| {}));
                    REGISTRY.with(|r| r.invoke(1, value + 1));
                }
            })
        });
        REGISTRY.with(|r| r.invoke(1, 0));
        assert_eq!(
            depth.get(),
            0,
            "the re-registered callback runs, not the old one"
        );
    }

    #[test]
    fn remove_drops_the_callback() {
        thread_local! {
            static REGISTRY: CallbackRegistry<()> = CallbackRegistry::new();
        }
        let ran = Rc::new(Cell::new(0));
        let ran_clone = Rc::clone(&ran);
        REGISTRY.with(|r| r.insert(1, move |()| ran_clone.set(ran_clone.get() + 1)));
        assert_eq!(REGISTRY.with(CallbackRegistry::len), 1);

        REGISTRY.with(|r| r.remove(1));

        assert_eq!(REGISTRY.with(CallbackRegistry::len), 0);
        REGISTRY.with(|r| r.invoke(1, ()));
        assert_eq!(ran.get(), 0, "a removed callback must not run");
    }

    /// A callback that captures a non-`Send` value compiles: that is the whole
    /// point of the registry being thread-local.
    #[test]
    fn callback_registry_accepts_non_send_captures() {
        thread_local! {
            static REGISTRY: CallbackRegistry<()> = CallbackRegistry::new();
        }
        let not_send = Rc::new(7);
        REGISTRY.with(|r| r.insert(1, move |()| assert_eq!(*not_send, 7)));
        REGISTRY.with(|r| r.invoke(1, ()));
    }
}
