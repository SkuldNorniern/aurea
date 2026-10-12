#ifndef NATIVE_GUI_WINDOWS_MENU_H
#define NATIVE_GUI_WINDOWS_MENU_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGMenuHandle ng_windows_create_menu(void);
void ng_windows_destroy_menu(NGMenuHandle handle);
int ng_windows_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_windows_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_windows_add_menu_separator(NGMenuHandle menu);
NGMenuHandle ng_windows_create_submenu(NGMenuHandle parent_menu, const char* title);
int ng_windows_add_menu_check_item(NGMenuHandle menu, const char* title, unsigned int id, int checked, int radio);
int ng_windows_menu_item_set_enabled(NGMenuHandle menu, unsigned int id, int enabled);
int ng_windows_menu_item_set_checked(NGMenuHandle menu, unsigned int id, int checked);
int ng_windows_menu_item_set_title(NGMenuHandle menu, unsigned int id, const char* title);
int ng_windows_menu_item_remove(NGMenuHandle menu, unsigned int id);
int ng_windows_menu_clear(NGMenuHandle menu);
int ng_windows_handle_menu_shortcut(NGHandle window, unsigned int keycode, unsigned int modifiers);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_MENU_H
