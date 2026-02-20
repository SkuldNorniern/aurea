//! Interior mutability helpers.
//!
//! Mutex lock is used with unwrap because we never panic while holding a lock,
//! so the mutex cannot become poisoned. See lib.rs implementation note.

use std::sync::{Mutex, MutexGuard};

/// Locks the mutex.
///
/// SAFETY: Unwrap is safe because we never panic while holding any lock in this crate.
/// Therefore the mutex cannot become poisoned.
#[inline(always)]
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // SAFETY: We never panic while holding any lock; mutex cannot be poisoned.
    m.lock().unwrap()
}
