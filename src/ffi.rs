use std::os::raw::{c_char, c_int, c_void};

#[allow(clippy::missing_safety_doc)]
unsafe extern "C" {
    // Platform initialization
    pub(crate) fn ng_platform_init() -> c_int;
    pub(crate) fn ng_platform_cleanup();

    // Platform runner
    pub(crate) fn ng_platform_run() -> c_int;
    
    // Window elements
    pub(crate) fn ng_platform_create_window(title: *const c_char, width: c_int, height: c_int) -> *mut c_void;
    pub(crate) fn ng_platform_destroy_window(handle: *mut c_void);

    // Menu elements
    pub(crate) fn ng_platform_create_menu() -> *mut c_void;
    pub(crate) fn ng_platform_destroy_menu(handle: *mut c_void);
    pub(crate) fn ng_platform_attach_menu(window: *mut c_void, menu: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_add_menu_item(menu: *mut c_void, title: *const c_char, id: u32) -> c_int;

    // Submenu elements
    pub(crate) fn ng_platform_create_submenu(parent: *mut c_void, title: *const c_char) -> *mut c_void;

    // Button elements
    pub(crate) fn ng_platform_create_button(title: *const c_char, id: u32) -> *mut c_void;
    pub(crate) fn ng_platform_button_invalidate(button: *mut c_void);
    pub(crate) fn ng_platform_create_label(text: *const c_char) -> *mut c_void;
    pub(crate) fn ng_platform_label_invalidate(label: *mut c_void);
    pub(crate) fn ng_platform_create_box(is_vertical: c_int) -> *mut c_void;
    pub(crate) fn ng_platform_box_invalidate(box_handle: *mut c_void);
    pub(crate) fn ng_platform_box_add(box_handle: *mut c_void, element: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_set_window_content(window: *mut c_void, content: *mut c_void) -> c_int;

    // Text elements
    pub(crate) fn ng_platform_create_text_editor(id: u32) -> *mut c_void;
    pub(crate) fn ng_platform_text_editor_invalidate(text_editor: *mut c_void);
    pub(crate) fn ng_platform_create_text_view(is_editable: c_int, id: u32) -> *mut c_void;
    pub(crate) fn ng_platform_text_view_invalidate(text_view: *mut c_void);
    #[allow(dead_code)] // Reserved for future use
    pub(crate) fn ng_platform_create_text_field() -> *mut c_void;
    pub(crate) fn ng_platform_set_text_content(text_handle: *mut c_void, content: *const c_char) -> c_int;
    pub(crate) fn ng_platform_get_text_content(text_handle: *mut c_void) -> *mut c_char;
    pub(crate) fn ng_platform_free_text_content(content: *mut c_char);

    // Canvas elements
    pub(crate) fn ng_platform_create_canvas(width: c_int, height: c_int) -> *mut c_void;
    pub(crate) fn ng_platform_canvas_invalidate(canvas: *mut c_void);
    pub(crate) fn ng_platform_canvas_invalidate_rect(canvas: *mut c_void, x: f32, y: f32, width: f32, height: f32);
    pub(crate) fn ng_platform_canvas_update_buffer(
        canvas: *mut c_void,
        buffer: *const u8,
        size: u32,
        width: u32,
        height: u32,
    );
    pub(crate) fn ng_platform_canvas_get_size(canvas: *mut c_void, width: *mut u32, height: *mut u32);
    pub(crate) fn ng_platform_canvas_get_window(canvas: *mut c_void) -> *mut c_void;
    pub(crate) fn ng_platform_get_scale_factor(window: *mut c_void) -> f32;
    pub(crate) fn ng_platform_window_set_scale_factor_callback(window: *mut c_void, callback: extern "C" fn(*mut c_void, f32));
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