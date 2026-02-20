//! Frame queue for scheduling and processing redraws.
//!
//! Manages canvas registrations and frame callbacks for event-driven invalidation.

use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

type CanvasRedrawCallback = Arc<dyn Fn() -> Result<(), crate::AureaError> + Send + Sync>;
type FrameCallback = Arc<dyn Fn() + Send + Sync + 'static>;

static FRAME_SCHEDULED: AtomicBool = AtomicBool::new(false);
static CANVAS_REGISTRY: LazyLock<Mutex<HashMap<usize, CanvasRedrawCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static FRAME_CALLBACKS: LazyLock<Mutex<Vec<FrameCallback>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub struct FrameScheduler;

impl FrameScheduler {
    pub fn schedule() {
        FRAME_SCHEDULED.store(true, Ordering::Relaxed);
    }

    pub fn take() -> bool {
        FRAME_SCHEDULED.swap(false, Ordering::Relaxed)
    }

    pub fn is_scheduled() -> bool {
        FRAME_SCHEDULED.load(Ordering::Relaxed)
    }

    pub fn register_canvas(handle: *mut c_void, callback: CanvasRedrawCallback) {
        let mut registry = crate::sync::lock(&CANVAS_REGISTRY);
        registry.insert(handle as usize, callback);
    }

    pub fn unregister_canvas(handle: *mut c_void) {
        let mut registry = crate::sync::lock(&CANVAS_REGISTRY);
        registry.remove(&(handle as usize));
    }

    pub fn register_frame_callback<F>(callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut callbacks = crate::sync::lock(&FRAME_CALLBACKS);
        callbacks.push(Arc::new(callback));
    }

    pub fn process_frames() -> crate::AureaResult<()> {
        if !Self::take() {
            return Ok(());
        }

        let (canvas_callbacks, global_callbacks) = {
            let registry = crate::sync::lock(&CANVAS_REGISTRY);
            let global = crate::sync::lock(&FRAME_CALLBACKS);
            (
                registry.values().cloned().collect::<Vec<_>>(),
                global.clone(),
            )
        };

        for callback in canvas_callbacks {
            if let Err(e) = callback() {
                log::warn!("Canvas redraw error: {:?}", e);
            }
        }

        for callback in global_callbacks {
            callback();
        }

        Ok(())
    }
}
