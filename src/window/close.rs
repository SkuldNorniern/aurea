//! The close flags a window shares between its methods and its lifecycle listener.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub(super) struct CloseFlags {
    forced: AtomicBool,
    waiting: AtomicBool,
}

impl CloseFlags {
    pub(super) fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Marks the next will-close as one the window asked for, so it is not a request.
    pub(super) fn mark_forced(&self) {
        self.forced.store(true, Ordering::Release);
    }

    /// Consumes the forced mark: true when the will-close is the window's own.
    pub(super) fn take_forced(&self) -> bool {
        self.forced.swap(false, Ordering::AcqRel)
    }

    pub(super) fn is_waiting(&self) -> bool {
        self.waiting.load(Ordering::Acquire)
    }

    pub(super) fn set_waiting(&self, waiting: bool) {
        self.waiting.store(waiting, Ordering::Release);
    }

    #[cfg(test)]
    pub(super) fn is_forced(&self) -> bool {
        self.forced.load(Ordering::Acquire)
    }
}
