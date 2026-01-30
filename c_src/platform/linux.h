#ifndef NATIVE_GUI_LINUX_H
#define NATIVE_GUI_LINUX_H

#include "common/types.h"

// Linux/GTK-specific initialization
int ng_platform_init(void);

// Linux/GTK-specific cleanup
void ng_platform_cleanup(void);

// Platform-specific implementations
NGHandle ng_platform_create_window(const char* title, int width, int height);
void ng_platform_destroy_window(NGHandle handle);
int ng_platform_window_set_cursor_visible(NGHandle window, int visible);
int ng_platform_window_set_cursor_grab(NGHandle window, int mode);
NGMenuHandle ng_platform_create_menu(void);
void ng_platform_destroy_menu(NGMenuHandle handle);
int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);

// Canvas functions
NGHandle ng_platform_create_canvas(int width, int height);
void ng_platform_canvas_invalidate(NGHandle canvas);

#endif // NATIVE_GUI_LINUX_H 
