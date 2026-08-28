use super::callback::{CallbackRegistry, IdAllocator};

static MENU_ITEM_ID: IdAllocator = IdAllocator::new();
thread_local! {
    static MENU_CALLBACKS: CallbackRegistry<()> = CallbackRegistry::new();
}

pub fn next_menu_item_id() -> u32 {
    MENU_ITEM_ID.next()
}

pub fn register_menu_callback(id: u32, callback: impl Fn() + 'static) {
    MENU_CALLBACKS.with(|r| r.insert(id, move |()| callback()));
}

pub fn invoke_menu_callback(id: u32) {
    MENU_CALLBACKS.with(|r| r.invoke(id, ()));
}
