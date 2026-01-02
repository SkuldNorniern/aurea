#ifndef NATIVE_GUI_WINDOWS_H
#define NATIVE_GUI_WINDOWS_H

#include "common/types.h"

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

// Canvas functions
NGHandle ng_platform_create_canvas(int width, int height);
void ng_platform_canvas_invalidate(NGHandle canvas);
void ng_platform_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height);
void ng_platform_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height);

#endif // NATIVE_GUI_WINDOWS_H 
