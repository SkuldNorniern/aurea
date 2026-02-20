use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

type MenuCallback = Box<dyn Fn() + Send + Sync>;

static MENU_ITEM_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
static MENU_CALLBACKS: LazyLock<Mutex<HashMap<u32, MenuCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn next_menu_item_id() -> u32 {
    let mut id_guard = crate::sync::lock(&MENU_ITEM_ID);
    *id_guard += 1;
    *id_guard - 1
}

pub(crate) fn register_menu_callback(id: u32, callback: impl Fn() + Send + Sync + 'static) {
    let mut callbacks = crate::sync::lock(&MENU_CALLBACKS);
    callbacks.insert(id, Box::new(callback));
}

pub(crate) fn invoke_menu_callback(id: u32) {
    let callbacks = crate::sync::lock(&MENU_CALLBACKS);
    if let Some(callback) = callbacks.get(&id) {
        callback();
    }
}
