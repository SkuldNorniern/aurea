pub use aurea_runtime::FrameScheduler;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ng_process_frames() {
    let result = std::panic::catch_unwind(|| {
        crate::window::process_all_window_events();
        crate::window::process_all_window_updates();
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
        std::io::Write::flush(&mut std::io::stderr()).ok();
    }

    if !FrameScheduler::is_scheduled() {
        unsafe { crate::ffi::ng_platform_frame_idle() };
    }
}
