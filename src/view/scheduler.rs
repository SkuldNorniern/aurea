pub use crate::runtime::frame_queue::FrameScheduler;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ng_process_frames() {
    crate::window::process_all_window_events();
    crate::window::process_all_window_updates();

    if let Err(e) = FrameScheduler::process_frames() {
        log::warn!("Frame processing error: {:?}", e);
    }
}
