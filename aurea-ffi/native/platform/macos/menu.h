#ifndef NATIVE_GUI_MACOS_MENU_H
#define NATIVE_GUI_MACOS_MENU_H

#include "common/types.h"

#ifdef __OBJC__
@class NSMenu;
#endif

#ifdef __cplusplus
extern "C" {
#endif

NGMenuHandle ng_macos_create_menu(void);
void ng_macos_destroy_menu(NGMenuHandle handle);
int ng_macos_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_macos_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_macos_add_menu_separator(NGMenuHandle menu);
int ng_macos_add_menu_check_item(NGMenuHandle menu, const char* title, unsigned int id, int checked, int radio);
int ng_macos_menu_item_set_enabled(NGMenuHandle menu, unsigned int id, int enabled);
int ng_macos_menu_item_set_checked(NGMenuHandle menu, unsigned int id, int checked);
int ng_macos_menu_item_set_title(NGMenuHandle menu, unsigned int id, const char* title);
int ng_macos_menu_item_remove(NGMenuHandle menu, unsigned int id);
int ng_macos_menu_clear(NGMenuHandle menu);
NGMenuHandle ng_macos_create_submenu(NGMenuHandle parentMenu, const char* title);
#ifdef __OBJC__
NSMenu* ng_macos_standard_menu_bar(void);
#endif

#ifdef __cplusplus
}
#endif

#endif