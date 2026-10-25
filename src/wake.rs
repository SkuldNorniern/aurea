//! Waking a mobile host at a set time, such as a caret's next blink, while its frame loop idles.

use std::sync::Mutex;
use std::thread;
use std::time::Instant;

/// The wake already on its way, if any.
static PENDING: Mutex<Option<Instant>> = Mutex::new(None);

/// Calls `wake` at `at`, from another thread, unless a wake at or before then is already pending.
pub(crate) fn wake_at(at: Instant, wake: fn()) {
    let Ok(mut pending) = PENDING.lock() else {
        return;
    };
    if pending.is_some_and(|due| due <= at && due >= Instant::now()) {
        return;
    }
    *pending = Some(at);
    drop(pending);
    let spawned = thread::Builder::new()
        .name("aurea-wake".into())
        .spawn(move || {
            thread::sleep(at.saturating_duration_since(Instant::now()));
            if let Ok(mut pending) = PENDING.lock()
                && *pending == Some(at)
            {
                *pending = None;
            }
            wake();
        });
    if let Err(e) = spawned {
        log::debug!("wake thread: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    static WOKEN: AtomicU32 = AtomicU32::new(0);

    fn count() {
        WOKEN.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn a_later_wake_waits_for_the_pending_one() {
        let now = Instant::now();
        wake_at(now + Duration::from_millis(20), count);
        wake_at(now + Duration::from_millis(40), count);
        thread::sleep(Duration::from_millis(120));
        assert_eq!(WOKEN.load(Ordering::SeqCst), 1);
    }
}
