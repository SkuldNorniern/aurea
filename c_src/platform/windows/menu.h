#ifndef NATIVE_GUI_WINDOWS_MENU_H
#define NATIVE_GUI_WINDOWS_MENU_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGMenuHandle ng_windows_create_menu(void);
void ng_windows_destroy_menu(NGMenuHandle handle);
int ng_windows_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_windows_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
NGMenuHandle ng_windows_create_submenu(NGMenuHandle parent_menu, const char* title);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_MENU_H

