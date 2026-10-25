//! Telling a mobile app when its host's GPU is up, so it can build its own GPU resources then.

use std::cell::RefCell;
use std::mem;
use std::sync::Arc;

use crate::render::ZenGpuContext;

type Ready = Box<dyn FnOnce(Arc<ZenGpuContext>)>;

thread_local! {
    /// The host's GPU once made, and who waits for it.
    static GPU: RefCell<(Option<Arc<ZenGpuContext>>, Vec<Ready>)> =
        const { RefCell::new((None, Vec::new())) };
}

/// Calls `ready` with the host's GPU: now if it is up, or when the host first makes it. It is never
/// called when the host draws on the CPU.
pub(crate) fn on_ready(ready: impl FnOnce(Arc<ZenGpuContext>) + 'static) {
    match GPU.with_borrow(|(context, _)| context.clone()) {
        Some(context) => ready(context),
        None => GPU.with_borrow_mut(|(_, waiting)| waiting.push(Box::new(ready))),
    }
}

/// Records the host's GPU and calls everyone waiting for it.
pub(crate) fn made(context: &Arc<ZenGpuContext>) {
    let waiting = GPU.with_borrow_mut(|(slot, waiting)| {
        *slot = Some(Arc::clone(context));
        mem::take(waiting)
    });
    for ready in waiting {
        ready(Arc::clone(context));
    }
}
