#ifndef NATIVE_GUI_LINUX_MENU_H
#define NATIVE_GUI_LINUX_MENU_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGMenuHandle ng_linux_create_menu(void);
void ng_linux_destroy_menu(NGMenuHandle handle);
int ng_linux_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_linux_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_linux_add_menu_separator(NGMenuHandle menu);
NGMenuHandle ng_linux_create_submenu(NGMenuHandle parent_menu, const char* title);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_LINUX_MENU_H
