use super::callback::{CallbackRegistry, IdAllocator};

static BUTTON_ID: IdAllocator = IdAllocator::new();
static BUTTON_CALLBACKS: CallbackRegistry<()> = CallbackRegistry::new();

static TEXT_EDITOR_ID: IdAllocator = IdAllocator::new();
static TEXT_EDITOR_CALLBACKS: CallbackRegistry<String> = CallbackRegistry::new();

static TEXT_VIEW_ID: IdAllocator = IdAllocator::new();
static TEXT_VIEW_CALLBACKS: CallbackRegistry<String> = CallbackRegistry::new();

static TAB_ID: IdAllocator = IdAllocator::new();
static TAB_SELECTED_CALLBACKS: CallbackRegistry<i32> = CallbackRegistry::new();
static TAB_DETACH_CALLBACKS: CallbackRegistry<i32> = CallbackRegistry::new();

static SIDEBAR_ID: IdAllocator = IdAllocator::new();
static SIDEBAR_SELECTED_CALLBACKS: CallbackRegistry<i32> = CallbackRegistry::new();

pub fn next_button_id() -> u32 {
    BUTTON_ID.next()
}

pub fn register_button_callback(id: u32, callback: impl Fn() + Send + Sync + 'static) {
    BUTTON_CALLBACKS.insert(id, move |()| callback());
}

pub fn invoke_button_callback(id: u32) {
    BUTTON_CALLBACKS.invoke(id, ());
}

pub fn next_text_editor_id() -> u32 {
    TEXT_EDITOR_ID.next()
}

pub fn register_text_editor_callback(id: u32, callback: impl Fn(String) + Send + Sync + 'static) {
    TEXT_EDITOR_CALLBACKS.insert(id, callback);
}

pub fn invoke_text_editor_callback(id: u32, content: String) {
    TEXT_EDITOR_CALLBACKS.invoke(id, content);
}

pub fn next_text_view_id() -> u32 {
    TEXT_VIEW_ID.next()
}

pub fn register_text_view_callback(id: u32, callback: impl Fn(String) + Send + Sync + 'static) {
    TEXT_VIEW_CALLBACKS.insert(id, callback);
}

pub fn invoke_text_view_callback(id: u32, content: String) {
    TEXT_VIEW_CALLBACKS.invoke(id, content);
}

pub fn next_tab_id() -> u32 {
    TAB_ID.next()
}

pub fn register_tab_callbacks(
    id: u32,
    on_selected: impl Fn(i32) + Send + Sync + 'static,
    on_detach: impl Fn(i32) + Send + Sync + 'static,
) {
    TAB_SELECTED_CALLBACKS.insert(id, on_selected);
    TAB_DETACH_CALLBACKS.insert(id, on_detach);
}

pub fn invoke_tab_selected(id: u32, index: i32) {
    TAB_SELECTED_CALLBACKS.invoke(id, index);
}

pub fn invoke_tab_detach(id: u32, index: i32) {
    TAB_DETACH_CALLBACKS.invoke(id, index);
}

pub fn next_sidebar_id() -> u32 {
    SIDEBAR_ID.next()
}

pub fn register_sidebar_callback(id: u32, on_selected: impl Fn(i32) + Send + Sync + 'static) {
    SIDEBAR_SELECTED_CALLBACKS.insert(id, on_selected);
}

pub fn invoke_sidebar_selected(id: u32, index: i32) {
    SIDEBAR_SELECTED_CALLBACKS.invoke(id, index);
}
