use std::os::raw::{c_char, c_int, c_void};

#[allow(clippy::missing_safety_doc)]
unsafe extern "C" {
    // Platform initialization
    pub(crate) fn ng_platform_init() -> c_int;
    pub(crate) fn ng_platform_cleanup();

    // Platform runner
    pub(crate) fn ng_platform_run() -> c_int;
    pub(crate) fn ng_platform_poll_events() -> c_int;

    // Window elements
    pub(crate) fn ng_platform_create_window(
        title: *const c_char,
        width: c_int,
        height: c_int,
    ) -> *mut c_void;
    
    pub(crate) fn ng_platform_create_window_with_type(
        title: *const c_char,
        width: c_int,
        height: c_int,
        window_type: c_int,
    ) -> *mut c_void;
    pub(crate) fn ng_platform_destroy_window(handle: *mut c_void);
    pub(crate) fn ng_platform_window_set_title(window: *mut c_void, title: *const c_char);
    pub(crate) fn ng_platform_window_set_size(window: *mut c_void, width: c_int, height: c_int);
    pub(crate) fn ng_platform_window_get_size(window: *mut c_void, width: *mut c_int, height: *mut c_int);
    pub(crate) fn ng_platform_window_request_close(window: *mut c_void);
    pub(crate) fn ng_platform_window_is_focused(window: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_window_get_content_view(window: *mut c_void) -> *mut c_void;
    pub(crate) fn ng_platform_window_show(window: *mut c_void);
    pub(crate) fn ng_platform_window_hide(window: *mut c_void);
    pub(crate) fn ng_platform_window_is_visible(window: *mut c_void) -> c_int;

    // Menu elements
    pub(crate) fn ng_platform_create_menu() -> *mut c_void;
    pub(crate) fn ng_platform_destroy_menu(handle: *mut c_void);
    pub(crate) fn ng_platform_attach_menu(window: *mut c_void, menu: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_add_menu_item(
        menu: *mut c_void,
        title: *const c_char,
        id: u32,
    ) -> c_int;

    // Submenu elements
    pub(crate) fn ng_platform_create_submenu(
        parent: *mut c_void,
        title: *const c_char,
    ) -> *mut c_void;

    // Button elements
    pub(crate) fn ng_platform_create_button(title: *const c_char, id: u32) -> *mut c_void;
    pub(crate) fn ng_platform_button_invalidate(button: *mut c_void);
    pub(crate) fn ng_platform_create_label(text: *const c_char) -> *mut c_void;
    pub(crate) fn ng_platform_label_invalidate(label: *mut c_void);
    pub(crate) fn ng_platform_create_box(is_vertical: c_int) -> *mut c_void;
    pub(crate) fn ng_platform_box_invalidate(box_handle: *mut c_void);
    pub(crate) fn ng_platform_box_add(box_handle: *mut c_void, element: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_set_window_content(
        window: *mut c_void,
        content: *mut c_void,
    ) -> c_int;

    // Text elements
    pub(crate) fn ng_platform_create_text_editor(id: u32) -> *mut c_void;
    pub(crate) fn ng_platform_text_editor_invalidate(text_editor: *mut c_void);
    pub(crate) fn ng_platform_create_text_view(is_editable: c_int, id: u32) -> *mut c_void;
    pub(crate) fn ng_platform_text_view_invalidate(text_view: *mut c_void);
    #[allow(dead_code)] // Reserved for future use
    pub(crate) fn ng_platform_create_text_field() -> *mut c_void;
    pub(crate) fn ng_platform_set_text_content(
        text_handle: *mut c_void,
        content: *const c_char,
    ) -> c_int;
    pub(crate) fn ng_platform_get_text_content(text_handle: *mut c_void) -> *mut c_char;
    pub(crate) fn ng_platform_free_text_content(content: *mut c_char);

    // Canvas elements
    pub(crate) fn ng_platform_create_canvas(width: c_int, height: c_int) -> *mut c_void;
    pub(crate) fn ng_platform_canvas_invalidate(canvas: *mut c_void);
    pub(crate) fn ng_platform_canvas_invalidate_rect(
        canvas: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    );
    pub(crate) fn ng_platform_canvas_update_buffer(
        canvas: *mut c_void,
        buffer: *const u8,
        size: u32,
        width: u32,
        height: u32,
    );
    pub(crate) fn ng_platform_canvas_get_size(
        canvas: *mut c_void,
        width: *mut u32,
        height: *mut u32,
    );
    pub(crate) fn ng_platform_canvas_get_window(canvas: *mut c_void) -> *mut c_void;
    pub(crate) fn ng_platform_get_scale_factor(window: *mut c_void) -> f32;
    pub(crate) fn ng_platform_window_set_scale_factor_callback(
        window: *mut c_void,
        callback: extern "C" fn(*mut c_void, f32),
    );

    // Lifecycle events
    pub(crate) fn ng_platform_window_set_lifecycle_callback(window: *mut c_void);

    // Frame processing
    pub(crate) fn ng_process_frames();

    // ImageView functions
    pub(crate) fn ng_platform_create_image_view() -> *mut c_void;
    pub(crate) fn ng_platform_image_view_load_from_path(
        image_view: *mut c_void,
        path: *const c_char,
    ) -> c_int;
    pub(crate) fn ng_platform_image_view_load_from_data(
        image_view: *mut c_void,
        data: *const u8,
        size: u32,
    ) -> c_int;
    pub(crate) fn ng_platform_image_view_set_scaling(image_view: *mut c_void, scaling_mode: c_int);
    pub(crate) fn ng_platform_image_view_invalidate(image_view: *mut c_void);

    // Slider functions
    pub(crate) fn ng_platform_create_slider(min: f64, max: f64) -> *mut c_void;
    pub(crate) fn ng_platform_slider_set_value(slider: *mut c_void, value: f64) -> c_int;
    pub(crate) fn ng_platform_slider_get_value(slider: *mut c_void) -> f64;
    pub(crate) fn ng_platform_slider_set_enabled(slider: *mut c_void, enabled: c_int) -> c_int;
    pub(crate) fn ng_platform_slider_invalidate(slider: *mut c_void);

    // Checkbox functions
    pub(crate) fn ng_platform_create_checkbox(label: *const c_char) -> *mut c_void;
    pub(crate) fn ng_platform_checkbox_set_checked(checkbox: *mut c_void, checked: c_int) -> c_int;
    pub(crate) fn ng_platform_checkbox_get_checked(checkbox: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_checkbox_set_enabled(checkbox: *mut c_void, enabled: c_int) -> c_int;
    pub(crate) fn ng_platform_checkbox_invalidate(checkbox: *mut c_void);

    // ProgressBar functions
    pub(crate) fn ng_platform_create_progress_bar() -> *mut c_void;
    pub(crate) fn ng_platform_progress_bar_set_value(
        progress_bar: *mut c_void,
        value: f64,
    ) -> c_int;
    pub(crate) fn ng_platform_progress_bar_set_indeterminate(
        progress_bar: *mut c_void,
        indeterminate: c_int,
    ) -> c_int;
    pub(crate) fn ng_platform_progress_bar_set_enabled(
        progress_bar: *mut c_void,
        enabled: c_int,
    ) -> c_int;
    pub(crate) fn ng_platform_progress_bar_invalidate(progress_bar: *mut c_void);

    // ComboBox functions
    pub(crate) fn ng_platform_create_combo_box() -> *mut c_void;
    pub(crate) fn ng_platform_combo_box_add_item(
        combo_box: *mut c_void,
        item: *const c_char,
    ) -> c_int;
    pub(crate) fn ng_platform_combo_box_set_selected(combo_box: *mut c_void, index: c_int)
    -> c_int;
    pub(crate) fn ng_platform_combo_box_get_selected(combo_box: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_combo_box_clear(combo_box: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_combo_box_set_enabled(
        combo_box: *mut c_void,
        enabled: c_int,
    ) -> c_int;
    pub(crate) fn ng_platform_combo_box_invalidate(combo_box: *mut c_void);
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_error(msg: *const c_char) {
    if !msg.is_null() {
        unsafe {
            let c_str = std::ffi::CStr::from_ptr(msg);
            if let Ok(s) = c_str.to_str() {
                log::error!("{}", s);
            }
        }
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_warn(msg: *const c_char) {
    if !msg.is_null() {
        unsafe {
            let c_str = std::ffi::CStr::from_ptr(msg);
            if let Ok(s) = c_str.to_str() {
                log::warn!("{}", s);
            }
        }
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_info(msg: *const c_char) {
    if !msg.is_null() {
        unsafe {
            let c_str = std::ffi::CStr::from_ptr(msg);
            if let Ok(s) = c_str.to_str() {
                log::info!("{}", s);
            }
        }
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_debug(msg: *const c_char) {
    if !msg.is_null() {
        unsafe {
            let c_str = std::ffi::CStr::from_ptr(msg);
            if let Ok(s) = c_str.to_str() {
                log::debug!("{}", s);
            }
        }
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn ng_log_trace(msg: *const c_char) {
    if !msg.is_null() {
        unsafe {
            let c_str = std::ffi::CStr::from_ptr(msg);
            if let Ok(s) = c_str.to_str() {
                log::trace!("{}", s);
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_menu_callback(id: u32) {
    crate::menu::invoke_menu_callback(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_button_callback(id: u32) {
    crate::elements::invoke_button_callback(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_text_callback(id: u32, content: *const c_char) {
    if !content.is_null() {
        unsafe {
            let c_str = std::ffi::CStr::from_ptr(content);
            if let Ok(s) = c_str.to_str() {
                crate::elements::invoke_text_callback(id, s.to_string());
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_textview_callback(id: u32, content: *const c_char) {
    if !content.is_null() {
        unsafe {
            let c_str = std::ffi::CStr::from_ptr(content);
            if let Ok(s) = c_str.to_str() {
                crate::elements::invoke_textview_callback(id, s.to_string());
            }
        }
    }
}

/// Invoke a lifecycle callback from the platform layer.
///
/// This function is called by platform-specific code when a lifecycle event occurs.
/// The event_id corresponds to the LifecycleEvent enum values.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_lifecycle_callback(window: *mut c_void, event_id: u32) {
    use crate::lifecycle::{LifecycleEvent, invoke_lifecycle_callback};
    let event = match event_id {
        0 => LifecycleEvent::ApplicationDidEnterBackground,
        1 => LifecycleEvent::ApplicationWillEnterForeground,
        2 => LifecycleEvent::ApplicationPaused,
        3 => LifecycleEvent::ApplicationResumed,
        4 => LifecycleEvent::ApplicationDestroyed,
        5 => LifecycleEvent::WindowWillClose,
        6 => LifecycleEvent::WindowMinimized,
        7 => LifecycleEvent::WindowRestored,
        8 => LifecycleEvent::MemoryWarning,
        9 => LifecycleEvent::SurfaceLost,
        10 => LifecycleEvent::SurfaceRecreated,
        _ => return, // Unknown event ID
    };

    invoke_lifecycle_callback(window, event);
}
