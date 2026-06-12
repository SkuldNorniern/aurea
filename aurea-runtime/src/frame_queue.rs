//! Frame queue for scheduling and processing redraws.

use aurea_foundation::AureaError;
use std::collections::{HashMap, HashSet};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

type CanvasRedrawCallback = Arc<dyn Fn() -> Result<(), AureaError> + Send + Sync>;
type FrameCallback = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameCallbackId(u64);

static FRAME_SCHEDULED: AtomicBool = AtomicBool::new(false);
static ALL_CANVASES_SCHEDULED: AtomicBool = AtomicBool::new(false);
static CANVAS_REGISTRY: LazyLock<Mutex<Arc<HashMap<usize, CanvasRedrawCallback>>>> =
    LazyLock::new(|| Mutex::new(Arc::new(HashMap::new())));
static PENDING_CANVASES: LazyLock<Mutex<HashSet<usize>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static FRAME_CALLBACK_COUNTER: AtomicU64 = AtomicU64::new(0);
static FRAME_CALLBACKS: LazyLock<Mutex<Arc<HashMap<FrameCallbackId, FrameCallback>>>> =
    LazyLock::new(|| Mutex::new(Arc::new(HashMap::new())));
static REQUEST_FRAME_HOOK: LazyLock<Mutex<Option<Box<dyn Fn() + Send + Sync>>>> =
    LazyLock::new(|| Mutex::new(None));

pub struct FrameScheduler;

impl FrameScheduler {
    pub fn set_request_frame_hook<F: Fn() + Send + Sync + 'static>(f: F) {
        *aurea_foundation::lock(&REQUEST_FRAME_HOOK) = Some(Box::new(f));
    }

    fn notify_platform() {
        if let Some(hook) = aurea_foundation::lock(&REQUEST_FRAME_HOOK).as_ref() {
            hook();
        }
    }

    pub fn schedule() {
        ALL_CANVASES_SCHEDULED.store(true, Ordering::Relaxed);
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        Self::notify_platform();
    }

    pub fn schedule_canvas(handle: *mut c_void) {
        let mut pending = aurea_foundation::lock(&PENDING_CANVASES);
        pending.insert(handle as usize);
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
        drop(pending);
        Self::notify_platform();
    }

    pub fn take() -> bool {
        FRAME_SCHEDULED.swap(false, Ordering::Relaxed)
    }

    pub fn is_scheduled() -> bool {
        FRAME_SCHEDULED.load(Ordering::Relaxed)
    }

    pub fn register_canvas(handle: *mut c_void, callback: CanvasRedrawCallback) {
        let mut registry = aurea_foundation::lock(&CANVAS_REGISTRY);
        let mut updated = (**registry).clone();
        updated.insert(handle as usize, callback);
        *registry = Arc::new(updated);
    }

    pub fn unregister_canvas(handle: *mut c_void) {
        let mut registry = aurea_foundation::lock(&CANVAS_REGISTRY);
        let mut updated = (**registry).clone();
        updated.remove(&(handle as usize));
        *registry = Arc::new(updated);
        aurea_foundation::lock(&PENDING_CANVASES).remove(&(handle as usize));
    }

    pub fn register_frame_callback<F>(callback: F) -> FrameCallbackId
    where
        F: Fn() + Send + Sync + 'static,
    {
        let id = FrameCallbackId(FRAME_CALLBACK_COUNTER.fetch_add(1, Ordering::Relaxed));
        let mut callbacks = aurea_foundation::lock(&FRAME_CALLBACKS);
        let mut updated = (**callbacks).clone();
        updated.insert(id, Arc::new(callback));
        *callbacks = Arc::new(updated);
        id
    }

    pub fn unregister_frame_callback(id: FrameCallbackId) {
        let mut callbacks = aurea_foundation::lock(&FRAME_CALLBACKS);
        let mut updated = (**callbacks).clone();
        updated.remove(&id);
        *callbacks = Arc::new(updated);
    }

    pub fn process_frames() -> Result<(), AureaError> {
        if !Self::take() {
            return Ok(());
        }

        let process_all_canvases = ALL_CANVASES_SCHEDULED.swap(false, Ordering::Relaxed);

        // Cheap Arc clones instead of cloning the whole registry/callback Vec
        // every frame; the locks are released before invoking callbacks
        // (which may re-register canvases or frame callbacks).
        let registry = aurea_foundation::lock(&CANVAS_REGISTRY).clone();
        let global_callbacks = aurea_foundation::lock(&FRAME_CALLBACKS).clone();

        let pending_handles = {
            let mut pending = aurea_foundation::lock(&PENDING_CANVASES);
            if process_all_canvases || pending.is_empty() {
                pending.clear();
                None
            } else {
                Some(pending.drain().collect::<Vec<_>>())
            }
        };

        match pending_handles {
            None => {
                for callback in registry.values() {
                    if let Err(e) = callback() {
                        log::warn!("Canvas redraw error: {:?}", e);
                    }
                }
            }
            Some(handles) => {
                for handle in handles {
                    if let Some(callback) = registry.get(&handle) {
                        if let Err(e) = callback() {
                            log::warn!("Canvas redraw error: {:?}", e);
                        }
                    }
                }
            }
        }

        for (_, callback) in global_callbacks.iter() {
            callback();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::LazyLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn handle(id: usize) -> *mut c_void {
        id as *mut c_void
    }

    #[test]
    fn targeted_schedule_processes_only_pending_canvas() {
        let _guard = aurea_foundation::lock(&TEST_LOCK);
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));

        let first_count = first.clone();
        FrameScheduler::register_canvas(
            handle(1),
            Arc::new(move || {
                first_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        let second_count = second.clone();
        FrameScheduler::register_canvas(
            handle(2),
            Arc::new(move || {
                second_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        FrameScheduler::schedule_canvas(handle(1));
        FrameScheduler::process_frames().unwrap();

        assert_eq!(first.load(Ordering::Relaxed), 1);
        assert_eq!(second.load(Ordering::Relaxed), 0);

        FrameScheduler::unregister_canvas(handle(1));
        FrameScheduler::unregister_canvas(handle(2));
    }

    #[test]
    fn global_schedule_processes_all_canvases() {
        let _guard = aurea_foundation::lock(&TEST_LOCK);
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));

        let first_count = first.clone();
        FrameScheduler::register_canvas(
            handle(3),
            Arc::new(move || {
                first_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        let second_count = second.clone();
        FrameScheduler::register_canvas(
            handle(4),
            Arc::new(move || {
                second_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        FrameScheduler::schedule_canvas(handle(3));
        FrameScheduler::schedule();
        FrameScheduler::process_frames().unwrap();

        assert_eq!(first.load(Ordering::Relaxed), 1);
        assert_eq!(second.load(Ordering::Relaxed), 1);

        FrameScheduler::unregister_canvas(handle(3));
        FrameScheduler::unregister_canvas(handle(4));
    }
}
