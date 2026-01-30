use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

static FRAME_SCHEDULED: AtomicBool = AtomicBool::new(false);

// Canvas registry: maps handle to redraw callback
// The callback can safely call redraw_if_needed on the canvas
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

    /// Register a canvas for automatic redraw when frames are scheduled
    /// The callback will be called when a frame needs to be drawn
    /// Also used for other elements that need frame-based updates (e.g., animated progress bars)
    pub(crate) fn register_canvas(handle: *mut c_void, callback: CanvasRedrawCallback) {
        let mut registry = CANVAS_REGISTRY.lock().unwrap();
        registry.insert(handle as usize, callback);
    }

    /// Unregister a canvas (called when canvas is dropped)
    /// Also used to unregister other frame-based update callbacks
    pub(crate) fn unregister_canvas(handle: *mut c_void) {
        let mut registry = CANVAS_REGISTRY.lock().unwrap();
        registry.remove(&(handle as usize));
    }

    /// Register a global frame callback that will be called on every frame
    pub fn register_frame_callback<F>(callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut callbacks = FRAME_CALLBACKS.lock().unwrap();
        callbacks.push(Arc::new(callback));
    }

    /// Process scheduled frames by calling redraw callbacks on all registered canvases
    /// This should be called from the event loop or window's frame handler
    pub fn process_frames() -> crate::AureaResult<()> {
        if !Self::take() {
            return Ok(());
        }

        // Get all callbacks (clone Arc to avoid holding lock during execution)
        let (canvas_callbacks, global_callbacks) = {
            let registry = CANVAS_REGISTRY.lock().unwrap();
            let global = FRAME_CALLBACKS.lock().unwrap();
            (
                registry.values().cloned().collect::<Vec<_>>(),
                global.clone(),
            )
        };

        // Execute all canvas callbacks
        for callback in canvas_callbacks {
            if let Err(e) = callback() {
                // Log error but continue with other canvases
                log::warn!("Canvas redraw error: {:?}", e);
            }
        }

        // Execute global frame callbacks
        for callback in global_callbacks {
            callback();
        }

        Ok(())
    }
}

// FFI function for platform to call frame processing
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ng_process_frames() {
    // Process window events first so state changes are handled before redraw
    crate::window::process_all_window_events();

    // Run per-window update callbacks before frame rendering
    crate::window::process_all_window_updates();

    if let Err(e) = FrameScheduler::process_frames() {
        log::warn!("Frame processing error: {:?}", e);
    }
}
