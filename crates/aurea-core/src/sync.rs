//! Interior mutability helpers.

use std::sync::{Mutex, MutexGuard};

/// Locks the mutex.
#[inline(always)]
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
