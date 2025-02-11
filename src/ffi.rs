use std::os::raw::{c_char, c_int, c_void};

#[allow(clippy::missing_safety_doc)]
extern "C" {
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
    pub(crate) fn ng_platform_create_button(title: *const c_char) -> *mut c_void;
    pub(crate) fn ng_platform_create_label(text: *const c_char) -> *mut c_void;
    pub(crate) fn ng_platform_create_box(is_vertical: c_int) -> *mut c_void;
    pub(crate) fn ng_platform_box_add(box_handle: *mut c_void, element: *mut c_void) -> c_int;
    pub(crate) fn ng_platform_set_window_content(window: *mut c_void, content: *mut c_void) -> c_int;

    // Text elements
    pub(crate) fn ng_platform_create_text_editor() -> *mut c_void;
    pub(crate) fn ng_platform_create_text_view(is_editable: c_int) -> *mut c_void;
    pub(crate) fn ng_platform_set_text_content(text_handle: *mut c_void, content: *const c_char) -> c_int;
    pub(crate) fn ng_platform_get_text_content(text_handle: *mut c_void) -> *mut c_char;
    pub(crate) fn ng_platform_free_text_content(content: *mut c_char);
} 