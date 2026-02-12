//! Interior mutability helpers.

use std::sync::{Mutex, MutexGuard};

#[inline(always)]
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // SAFETY: We never panic while holding any lock; mutex cannot be poisoned.
    m.lock().unwrap()
}
