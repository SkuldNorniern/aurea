//! Callbacks for menu items, by id.

use super::callback::{CallbackRegistry, IdAllocator};

static MENU_ITEM_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static MENU_CALLBACKS: CallbackRegistry<()> = CallbackRegistry::new();
}

/// Allocates a menu item callback ID.
pub fn next_menu_item_id() -> u32 {
    MENU_ITEM_ID.next()
}

/// Registers `callback` for menu item `id`.
pub fn register_menu_callback(id: u32, callback: impl Fn() + 'static) {
    MENU_CALLBACKS.with(|r| r.insert(id, move |()| callback()));
}

/// Drops the menu callback registered for `id`.
pub fn unregister_menu_callback(id: u32) {
    MENU_CALLBACKS.with(|r| r.remove(id));
}

/// Runs the callback registered for menu item `id`, if any.
pub fn invoke_menu_callback(id: u32) {
    MENU_CALLBACKS.with(|r| r.invoke(id, ()));
}
