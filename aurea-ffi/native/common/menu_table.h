#ifndef NATIVE_GUI_MENU_TABLE_H
#define NATIVE_GUI_MENU_TABLE_H

/* For platforms that match menu shortcuts themselves: which menu bar each menu hangs under, the
   shortcuts each item's title names, and which menu bar each window shows. Handles are the
   platform's own menu and window handles. */

#ifdef __cplusplus
extern "C" {
#endif

/* `menu` hangs under the menu bar `root`; a menu bar is its own root. */
void ng_menu_table_set_root(void* menu, void* root);
/* The menu bar `menu` hangs under, or `menu` itself when unknown. */
void* ng_menu_table_root(void* menu);

/* Lists the shortcut `title` names, if any, for item `id` in `menu`. */
void ng_menu_table_add_shortcut(void* menu, const char* title, unsigned int id);
/* The next shortcut under `root` for this key, from `*at` on, which it moves past: the item's menu
   and id; 0 when there are no more. Start `*at` at 0. */
int ng_menu_table_next_shortcut(void* root, unsigned int keycode, unsigned int modifiers, int* at,
                                void** menu, unsigned int* id);
/* Drops item `id`'s shortcut. */
void ng_menu_table_forget_item(unsigned int id);
/* Drops `menu`: its root, the shortcuts in it or under it, and the windows showing it. */
void ng_menu_table_forget_menu(void* menu);

/* `window` shows the menu bar `root`. */
void ng_menu_table_bind_window(void* window, void* root);
/* The menu bar `window` shows, or NULL. */
void* ng_menu_table_window_root(void* window);
/* Drops `window`, which is going away. */
void ng_menu_table_forget_window(void* window);

#ifdef __cplusplus
}
#endif

#endif
