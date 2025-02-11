use std::os::raw::{c_char, c_int, c_void};

#[allow(clippy::missing_safety_doc)]
extern "C" {
    pub(crate) fn ng_platform_init() -> c_int;
    pub(crate) fn ng_platform_cleanup();
    pub(crate) fn ng_platform_create_window(title: *const c_char, width: c_int, height: c_int) -> *mut c_void;
    pub(crate) fn ng_platform_destroy_window(handle: *mut c_void);
    pub(crate) fn ng_platform_create_menu() -> *mut c_void;
    pub(crate) fn ng_platform_destroy_menu(handle: *mut c_void);
    pub(crate) fn ng_platform_attach_menu(window: *mut c_void, menu: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_add_menu_item(menu: *mut c_void, title: *const c_char, id: u32) -> c_int;
    pub(crate) fn ng_platform_run() -> c_int;
    pub(crate) fn ng_platform_create_submenu(parent: *mut c_void, title: *const c_char) -> *mut c_void;
    pub(crate) fn ng_platform_create_button(title: *const c_char) -> *mut c_void;
    pub(crate) fn ng_platform_create_label(text: *const c_char) -> *mut c_void;
    pub(crate) fn ng_platform_create_box(is_vertical: c_int) -> *mut c_void;
    pub(crate) fn ng_platform_box_add(box_handle: *mut c_void, element: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_set_window_content(window: *mut c_void, content: *mut c_void) -> c_int;
} 