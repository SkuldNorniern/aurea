use crate::ffi::ng_platform_frame_idle;
use crate::window::{process_all_window_events, process_all_window_updates};
pub use aurea_runtime::FrameScheduler;
use std::io::{Write, stderr};
use std::panic::catch_unwind;

#[unsafe(no_mangle)]
/// Process scheduled window and canvas frames from the native platform pump.
///
/// # Safety
///
/// This function is exported for the native platform layer. It must be called
/// from a thread where the platform event queue and UI objects may be accessed.
pub unsafe extern "C" fn ng_process_frames() {
    let result = catch_unwind(|| {
        process_all_window_events();
        process_all_window_updates();
        if let Err(e) = FrameScheduler::process_frames() {
            log::warn!("Frame processing error: {:?}", e);
        }
    });
    if let Err(e) = result {
        let msg = e
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| e.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic>".to_owned());
        eprintln!("[ng_process_frames] PANIC: {msg}");
        Write::flush(&mut stderr()).ok();
    }

    if !FrameScheduler::is_scheduled() {
        unsafe { ng_platform_frame_idle() };
    }
}
