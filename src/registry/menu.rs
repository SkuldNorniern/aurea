use super::callback::{CallbackRegistry, IdAllocator};

static MENU_ITEM_ID: IdAllocator = IdAllocator::new();
static MENU_CALLBACKS: CallbackRegistry<()> = CallbackRegistry::new();

pub fn next_menu_item_id() -> u32 {
    MENU_ITEM_ID.next()
}

pub fn register_menu_callback(id: u32, callback: impl Fn() + Send + Sync + 'static) {
    MENU_CALLBACKS.insert(id, move |()| callback());
}

pub fn invoke_menu_callback(id: u32) {
    MENU_CALLBACKS.invoke(id, ());
}
