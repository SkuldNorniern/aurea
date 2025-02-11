#ifndef NATIVE_GUI_MACOS_MENU_H
#define NATIVE_GUI_MACOS_MENU_H

#include "../../common/types.h"

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
NGMenuHandle ng_macos_create_submenu(NGMenuHandle parentMenu, const char* title);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_MENU_H 