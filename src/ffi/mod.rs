mod callbacks;
mod declarations;
mod logging;

pub use callbacks::*;
pub use declarations::*;
#[cfg(target_os = "linux")]
pub use declarations::{
    ng_platform_canvas_get_wayland_handle, ng_platform_canvas_get_xcb_handle,
    ng_platform_window_get_wayland_handle, ng_platform_window_get_xcb_handle,
};
pub use declarations::{
    ng_platform_create_window, ng_platform_create_window_with_type, ng_platform_destroy_window,
    ng_platform_poll_events, ng_platform_run, ng_platform_window_get_content_view,
    ng_platform_window_get_position, ng_platform_window_get_size, ng_platform_window_hide,
    ng_platform_window_is_focused, ng_platform_window_is_visible, ng_platform_window_request_close,
    ng_platform_window_set_cursor_grab, ng_platform_window_set_cursor_visible,
    ng_platform_window_set_position, ng_platform_window_set_size, ng_platform_window_set_title,
    ng_platform_window_show,
};
pub use logging::*;
