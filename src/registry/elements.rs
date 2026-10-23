//! Callbacks for buttons, tabs, sidebar lists and tables, by id.

use super::callback::{CallbackRegistry, IdAllocator};

static BUTTON_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static BUTTON_CALLBACKS: CallbackRegistry<()> = CallbackRegistry::new();
}

static TAB_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static TAB_SELECTED_CALLBACKS: CallbackRegistry<i32> = CallbackRegistry::new();
}
thread_local! {
    static TAB_DETACH_CALLBACKS: CallbackRegistry<i32> = CallbackRegistry::new();
}

static SIDEBAR_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static SIDEBAR_SELECTED_CALLBACKS: CallbackRegistry<i32> = CallbackRegistry::new();
}

static TABLE_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static TABLE_SELECTED_CALLBACKS: CallbackRegistry<i32> = CallbackRegistry::new();
    static TABLE_ACTIVATED_CALLBACKS: CallbackRegistry<i32> = CallbackRegistry::new();
}

/// Allocates a button callback ID.
pub fn next_button_id() -> u32 {
    BUTTON_ID.next()
}

/// Registers `callback` for button `id`.
pub fn register_button_callback(id: u32, callback: impl Fn() + 'static) {
    BUTTON_CALLBACKS.with(|r| r.insert(id, move |()| callback()));
}

/// Drops the button callback registered for `id`.
pub fn unregister_button_callback(id: u32) {
    BUTTON_CALLBACKS.with(|r| r.remove(id));
}

/// Runs the callback registered for button `id`, if any.
pub fn invoke_button_callback(id: u32) {
    BUTTON_CALLBACKS.with(|r| r.invoke(id, ()));
}

/// Allocates a tab bar callback ID.
pub fn next_tab_id() -> u32 {
    TAB_ID.next()
}

/// Registers selection and detach callbacks for tab bar `id`.
pub fn register_tab_callbacks(
    id: u32,
    on_selected: impl Fn(i32) + 'static,
    on_detach: impl Fn(i32) + 'static,
) {
    TAB_SELECTED_CALLBACKS.with(|r| r.insert(id, on_selected));
    TAB_DETACH_CALLBACKS.with(|r| r.insert(id, on_detach));
}

/// Drops both tab callbacks registered for `id`.
pub fn unregister_tab_callbacks(id: u32) {
    TAB_SELECTED_CALLBACKS.with(|r| r.remove(id));
    TAB_DETACH_CALLBACKS.with(|r| r.remove(id));
}

/// Runs the selection callback for tab bar `id`.
pub fn invoke_tab_selected(id: u32, index: i32) {
    TAB_SELECTED_CALLBACKS.with(|r| r.invoke(id, index));
}

/// Runs the detach callback for tab bar `id`.
pub fn invoke_tab_detach(id: u32, index: i32) {
    TAB_DETACH_CALLBACKS.with(|r| r.invoke(id, index));
}

/// Allocates a sidebar list callback ID.
pub fn next_sidebar_id() -> u32 {
    SIDEBAR_ID.next()
}

/// Registers the selection callback for sidebar list `id`.
pub fn register_sidebar_callback(id: u32, on_selected: impl Fn(i32) + 'static) {
    SIDEBAR_SELECTED_CALLBACKS.with(|r| r.insert(id, on_selected));
}

/// Drops the sidebar callback registered for `id`.
pub fn unregister_sidebar_callback(id: u32) {
    SIDEBAR_SELECTED_CALLBACKS.with(|r| r.remove(id));
}

/// Runs the selection callback for sidebar list `id`.
pub fn invoke_sidebar_selected(id: u32, index: i32) {
    SIDEBAR_SELECTED_CALLBACKS.with(|r| r.invoke(id, index));
}

/// Allocates a table callback ID.
pub fn next_table_id() -> u32 {
    TABLE_ID.next()
}

/// Registers the selection and activation callbacks for table `id`, by row index.
pub fn register_table_callbacks(
    id: u32,
    on_selected: impl Fn(i32) + 'static,
    on_activated: impl Fn(i32) + 'static,
) {
    TABLE_SELECTED_CALLBACKS.with(|r| r.insert(id, on_selected));
    TABLE_ACTIVATED_CALLBACKS.with(|r| r.insert(id, on_activated));
}

/// Drops both table callbacks registered for `id`.
pub fn unregister_table_callbacks(id: u32) {
    TABLE_SELECTED_CALLBACKS.with(|r| r.remove(id));
    TABLE_ACTIVATED_CALLBACKS.with(|r| r.remove(id));
}

/// Runs the selection callback for table `id`.
pub fn invoke_table_selected(id: u32, index: i32) {
    TABLE_SELECTED_CALLBACKS.with(|r| r.invoke(id, index));
}

/// Runs the activation callback for table `id`.
pub fn invoke_table_activated(id: u32, index: i32) {
    TABLE_ACTIVATED_CALLBACKS.with(|r| r.invoke(id, index));
}
