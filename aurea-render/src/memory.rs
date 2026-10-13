//! Best-effort replacement of large temporary pixel buffers after a resize.

use std::mem::size_of;

pub(crate) fn smaller_buffer<T>(capacity: usize, needed: usize) -> Option<Vec<T>> {
    if capacity <= needed.saturating_mul(4)
        || capacity
            .saturating_sub(needed)
            .saturating_mul(size_of::<T>())
            < (4 << 20)
    {
        return None;
    }
    let target = needed.checked_next_power_of_two().unwrap_or(needed);
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(target).ok()?;
    Some(buffer)
}
