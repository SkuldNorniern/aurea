use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, LazyLock, Arc};
use std::collections::HashMap;
use std::os::raw::c_void;

static FRAME_SCHEDULED: AtomicBool = AtomicBool::new(false);

type CanvasRedrawCallback = Arc<dyn Fn() -> Result<(), crate::AureaError> + Send + Sync>;

static CANVAS_REGISTRY: LazyLock<Mutex<HashMap<usize, CanvasRedrawCallback>>> = 
    LazyLock::new(|| Mutex::new(HashMap::new()));

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
    
    /// Process scheduled frames by calling redraw callbacks on all registered canvases This should
    /// be called from the event loop or window's frame handler.
    pub fn process_frames() -> crate::AureaResult<()> {
        if !Self::take() {
            return Ok(());
        }
        
        let callbacks: Vec<CanvasRedrawCallback> = {
            let registry = CANVAS_REGISTRY.lock().unwrap();
            registry.values().cloned().collect()
        };
        
        for callback in callbacks {
            if let Err(e) = callback() {
                log::warn!("Canvas redraw error: {:?}", e);
            }
        }
        
        Ok(())
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ng_process_frames() {
    if let Err(e) = FrameScheduler::process_frames() {
        log::warn!("Frame processing error: {:?}", e);
    }
}

