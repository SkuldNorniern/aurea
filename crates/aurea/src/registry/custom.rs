//! Custom callback registry for SwiftUI and other platform-triggered actions.

use std::sync::{LazyLock, Mutex};

type VoidCallback = Box<dyn Fn() + Send + Sync>;

static CUSTOM_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
static CUSTOM_CALLBACKS: LazyLock<Mutex<std::collections::HashMap<u32, VoidCallback>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

pub fn next_custom_id() -> u32 {
    let mut guard = crate::sync::lock(&CUSTOM_ID);
    *guard += 1;
    *guard - 1
}

pub fn register_custom_callback(id: u32, callback: impl Fn() + Send + Sync + 'static) {
    let mut callbacks = crate::sync::lock(&CUSTOM_CALLBACKS);
    callbacks.insert(id, Box::new(callback));
}

pub fn invoke_custom_callback(id: u32) {
    let callbacks = crate::sync::lock(&CUSTOM_CALLBACKS);
    if let Some(cb) = callbacks.get(&id) {
        cb();
    }
}
