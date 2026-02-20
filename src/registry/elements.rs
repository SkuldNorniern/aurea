use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

type VoidCallback = Box<dyn Fn() + Send + Sync>;
type TextCallback = Box<dyn Fn(String) + Send + Sync>;
type IndexCallback = Box<dyn Fn(i32) + Send + Sync>;

static BUTTON_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
static BUTTON_CALLBACKS: LazyLock<Mutex<HashMap<u32, VoidCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static TEXT_EDITOR_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
static TEXT_EDITOR_CALLBACKS: LazyLock<Mutex<HashMap<u32, TextCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static TEXT_VIEW_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
static TEXT_VIEW_CALLBACKS: LazyLock<Mutex<HashMap<u32, TextCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static TAB_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
static TAB_SELECTED_CALLBACKS: LazyLock<Mutex<HashMap<u32, IndexCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static TAB_DETACH_CALLBACKS: LazyLock<Mutex<HashMap<u32, IndexCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static SIDEBAR_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
static SIDEBAR_SELECTED_CALLBACKS: LazyLock<Mutex<HashMap<u32, IndexCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn next_id(counter: &LazyLock<Mutex<u32>>) -> u32 {
    let mut id_guard = crate::sync::lock(counter);
    *id_guard += 1;
    *id_guard - 1
}

pub fn next_button_id() -> u32 {
    next_id(&BUTTON_ID)
}

pub fn register_button_callback(id: u32, callback: impl Fn() + Send + Sync + 'static) {
    let mut callbacks = crate::sync::lock(&BUTTON_CALLBACKS);
    callbacks.insert(id, Box::new(callback));
}

pub fn invoke_button_callback(id: u32) {
    let callbacks = crate::sync::lock(&BUTTON_CALLBACKS);
    if let Some(callback) = callbacks.get(&id) {
        callback();
    }
}

pub fn next_text_editor_id() -> u32 {
    next_id(&TEXT_EDITOR_ID)
}

pub fn register_text_editor_callback(
    id: u32,
    callback: impl Fn(String) + Send + Sync + 'static,
) {
    let mut callbacks = crate::sync::lock(&TEXT_EDITOR_CALLBACKS);
    callbacks.insert(id, Box::new(callback));
}

pub fn invoke_text_editor_callback(id: u32, content: String) {
    let callbacks = crate::sync::lock(&TEXT_EDITOR_CALLBACKS);
    if let Some(callback) = callbacks.get(&id) {
        callback(content);
    }
}

pub fn next_text_view_id() -> u32 {
    next_id(&TEXT_VIEW_ID)
}

pub fn register_text_view_callback(
    id: u32,
    callback: impl Fn(String) + Send + Sync + 'static,
) {
    let mut callbacks = crate::sync::lock(&TEXT_VIEW_CALLBACKS);
    callbacks.insert(id, Box::new(callback));
}

pub fn invoke_text_view_callback(id: u32, content: String) {
    let callbacks = crate::sync::lock(&TEXT_VIEW_CALLBACKS);
    if let Some(callback) = callbacks.get(&id) {
        callback(content);
    }
}

pub fn next_tab_id() -> u32 {
    next_id(&TAB_ID)
}

pub fn register_tab_callbacks(
    id: u32,
    on_selected: impl Fn(i32) + Send + Sync + 'static,
    on_detach: impl Fn(i32) + Send + Sync + 'static,
) {
    let mut selected = crate::sync::lock(&TAB_SELECTED_CALLBACKS);
    selected.insert(id, Box::new(on_selected));

    let mut detach = crate::sync::lock(&TAB_DETACH_CALLBACKS);
    detach.insert(id, Box::new(on_detach));
}

pub fn invoke_tab_selected(id: u32, index: i32) {
    let callbacks = crate::sync::lock(&TAB_SELECTED_CALLBACKS);
    if let Some(callback) = callbacks.get(&id) {
        callback(index);
    }
}

pub fn invoke_tab_detach(id: u32, index: i32) {
    let callbacks = crate::sync::lock(&TAB_DETACH_CALLBACKS);
    if let Some(callback) = callbacks.get(&id) {
        callback(index);
    }
}

pub fn next_sidebar_id() -> u32 {
    next_id(&SIDEBAR_ID)
}

pub fn register_sidebar_callback(
    id: u32,
    on_selected: impl Fn(i32) + Send + Sync + 'static,
) {
    let mut callbacks = crate::sync::lock(&SIDEBAR_SELECTED_CALLBACKS);
    callbacks.insert(id, Box::new(on_selected));
}

pub fn invoke_sidebar_selected(id: u32, index: i32) {
    let callbacks = crate::sync::lock(&SIDEBAR_SELECTED_CALLBACKS);
    if let Some(callback) = callbacks.get(&id) {
        callback(index);
    }
}
