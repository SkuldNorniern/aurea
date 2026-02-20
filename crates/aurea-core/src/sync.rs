//! Interior mutability helpers.

use std::sync::{Mutex, MutexGuard};

/// Locks the mutex.
///
/// SAFETY: Unwrap is safe because we never panic while holding any lock in this crate.
#[inline(always)]
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap()
}
