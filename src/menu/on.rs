//! The native menu calls, with `menu` on.

pub(super) use crate::ffi::{
    ng_platform_add_menu_check_item, ng_platform_add_menu_item, ng_platform_add_menu_separator,
    ng_platform_attach_menu, ng_platform_create_menu, ng_platform_create_submenu,
    ng_platform_destroy_menu, ng_platform_menu_clear, ng_platform_menu_item_remove,
    ng_platform_menu_item_set_checked, ng_platform_menu_item_set_enabled,
    ng_platform_menu_item_set_title, ng_platform_menu_move_last,
};
