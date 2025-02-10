#ifndef NATIVE_GUI_MACOS_H
#define NATIVE_GUI_MACOS_H

#include "../common/types.h"

#ifdef __OBJC__
@class NSWindow;
@class NSMenu;
#endif

// Platform-specific implementations
#ifdef __cplusplus
extern "C" {
#endif

int ng_platform_init(void);
void ng_platform_cleanup(void);
NGHandle ng_platform_create_window(const char* title, int width, int height);
void ng_platform_destroy_window(NGHandle handle);
NGMenuHandle ng_platform_create_menu(void);
void ng_platform_destroy_menu(NGMenuHandle handle);
int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_platform_run(void);
NGMenuHandle ng_platform_create_submenu(NGMenuHandle parentMenu, const char* title);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_H 