#ifndef NATIVE_GUI_WINDOWS_H
#define NATIVE_GUI_WINDOWS_H

#include "../common/types.h"

// Windows-specific initialization
int ng_platform_init(void);

// Windows-specific cleanup
void ng_platform_cleanup(void);

// Platform-specific implementations
NGHandle ng_platform_create_window(const char* title, int width, int height);
void ng_platform_destroy_window(NGHandle handle);
NGMenuHandle ng_platform_create_menu(void);
void ng_platform_destroy_menu(NGMenuHandle handle);
int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);

#endif // NATIVE_GUI_WINDOWS_H 