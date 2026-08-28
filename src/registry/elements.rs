use super::callback::{CallbackRegistry, IdAllocator};

static BUTTON_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static BUTTON_CALLBACKS: CallbackRegistry<()> = CallbackRegistry::new();
}

static TEXT_EDITOR_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static TEXT_EDITOR_CALLBACKS: CallbackRegistry<String> = CallbackRegistry::new();
}

static TEXT_VIEW_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static TEXT_VIEW_CALLBACKS: CallbackRegistry<String> = CallbackRegistry::new();
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

pub fn next_button_id() -> u32 {
    BUTTON_ID.next()
}

pub fn register_button_callback(id: u32, callback: impl Fn() + 'static) {
    BUTTON_CALLBACKS.with(|r| r.insert(id, move |()| callback()));
}

/// Drops the button callback registered for `id`.
pub fn unregister_button_callback(id: u32) {
    BUTTON_CALLBACKS.with(|r| r.remove(id));
}

pub fn invoke_button_callback(id: u32) {
    BUTTON_CALLBACKS.with(|r| r.invoke(id, ()));
}

pub fn next_text_editor_id() -> u32 {
    TEXT_EDITOR_ID.next()
}

pub fn register_text_editor_callback(id: u32, callback: impl Fn(String) + 'static) {
    TEXT_EDITOR_CALLBACKS.with(|r| r.insert(id, callback));
}

/// Drops the text editor callback registered for `id`.
pub fn unregister_text_editor_callback(id: u32) {
    TEXT_EDITOR_CALLBACKS.with(|r| r.remove(id));
}

pub fn invoke_text_editor_callback(id: u32, content: String) {
    TEXT_EDITOR_CALLBACKS.with(|r| r.invoke(id, content));
}

pub fn next_text_view_id() -> u32 {
    TEXT_VIEW_ID.next()
}

pub fn register_text_view_callback(id: u32, callback: impl Fn(String) + 'static) {
    TEXT_VIEW_CALLBACKS.with(|r| r.insert(id, callback));
}

/// Drops the text view callback registered for `id`.
pub fn unregister_text_view_callback(id: u32) {
    TEXT_VIEW_CALLBACKS.with(|r| r.remove(id));
}

pub fn invoke_text_view_callback(id: u32, content: String) {
    TEXT_VIEW_CALLBACKS.with(|r| r.invoke(id, content));
}

pub fn next_tab_id() -> u32 {
    TAB_ID.next()
}

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

pub fn invoke_tab_selected(id: u32, index: i32) {
    TAB_SELECTED_CALLBACKS.with(|r| r.invoke(id, index));
}

pub fn invoke_tab_detach(id: u32, index: i32) {
    TAB_DETACH_CALLBACKS.with(|r| r.invoke(id, index));
}

pub fn next_sidebar_id() -> u32 {
    SIDEBAR_ID.next()
}

pub fn register_sidebar_callback(id: u32, on_selected: impl Fn(i32) + 'static) {
    SIDEBAR_SELECTED_CALLBACKS.with(|r| r.insert(id, on_selected));
}

/// Drops the sidebar callback registered for `id`.
pub fn unregister_sidebar_callback(id: u32) {
    SIDEBAR_SELECTED_CALLBACKS.with(|r| r.remove(id));
}

pub fn invoke_sidebar_selected(id: u32, index: i32) {
    SIDEBAR_SELECTED_CALLBACKS.with(|r| r.invoke(id, index));
}
