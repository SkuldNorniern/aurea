use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

static FRAME_SCHEDULED: AtomicBool = AtomicBool::new(false);

type CanvasRedrawCallback = Arc<dyn Fn() -> Result<(), crate::AureaError> + Send + Sync>;
type FrameCallback = Arc<dyn Fn() + Send + Sync + 'static>;

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

    pub(crate) fn register_canvas(handle: *mut c_void, callback: CanvasRedrawCallback) {
        let mut registry = CANVAS_REGISTRY.lock().unwrap();
        registry.insert(handle as usize, callback);
    }

    pub(crate) fn unregister_canvas(handle: *mut c_void) {
        let mut registry = CANVAS_REGISTRY.lock().unwrap();
        registry.remove(&(handle as usize));
    }

    /// Register a global frame callback that will be called on every frame.
    pub fn register_frame_callback<F>(callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut callbacks = FRAME_CALLBACKS.lock().unwrap();
        callbacks.push(Arc::new(callback));
    }

    /// Process scheduled frames by calling redraw callbacks on all registered canvases This should
    /// be called from the event loop or window's frame handler.
    pub fn process_frames() -> crate::AureaResult<()> {
        if !Self::take() {
            return Ok(());
        }

        let (canvas_callbacks, global_callbacks) = {
            let registry = CANVAS_REGISTRY.lock().unwrap();
            let global = FRAME_CALLBACKS.lock().unwrap();
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ng_process_frames() {
    crate::window::process_all_window_events();

    crate::window::process_all_window_updates();

    if let Err(e) = FrameScheduler::process_frames() {
        log::warn!("Frame processing error: {:?}", e);
    }
}
