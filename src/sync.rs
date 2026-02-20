//! Interior mutability helpers.

use std::sync::{Mutex, MutexGuard};

/// Locks the mutex.
#[inline(always)]
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // SAFETY: We never panic while holding any lock; mutex cannot be poisoned.
    m.lock().unwrap()
}
