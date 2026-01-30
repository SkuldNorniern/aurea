#ifndef NATIVE_GUI_H
#define NATIVE_GUI_H

#include "common/types.h"
#include <stdint.h>
#include "common/errors.h"

#ifdef __cplusplus
extern "C" {
#endif

// Initialize the native GUI system
int ng_init(void);

// Cleanup the native GUI system
void ng_cleanup(void);

// Core platform-agnostic functions
int ng_add_raw_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_attach_menu_to_window(NGHandle window, NGMenuHandle menu);
NGMenuHandle ng_create_menu_handle(void);
NGHandle ng_create_window(const char* title, int width, int height);
void ng_destroy_menu_handle(NGMenuHandle handle);
void ng_destroy_window(NGHandle handle);
int ng_handle_menu_event(NGMenuHandle menu, unsigned int id);

// Platform-specific functions (implemented in platform/*.c)
int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu);
void ng_platform_cleanup(void);
NGMenuHandle ng_platform_create_menu(void);
NGHandle ng_platform_create_window(const char* title, int width, int height);
void ng_platform_destroy_menu(NGHandle handle);
void ng_platform_destroy_window(NGHandle handle);
void ng_platform_window_show(NGHandle window);
void ng_platform_window_hide(NGHandle window);
int ng_platform_window_is_visible(NGHandle window);
int ng_platform_window_set_cursor_visible(NGHandle window, int visible);
int ng_platform_window_set_cursor_grab(NGHandle window, int mode);
int ng_platform_window_get_xcb_handle(NGHandle window, uint32_t* xcb_window, void** xcb_connection);
int ng_platform_window_get_wayland_handle(NGHandle window, void** surface, void** display);
int ng_platform_canvas_get_xcb_handle(NGHandle canvas, uint32_t* xcb_window, void** xcb_connection);
int ng_platform_canvas_get_wayland_handle(NGHandle canvas, void** surface, void** display);
int ng_platform_handle_menu_event(NGMenuHandle menu, unsigned int id);
int ng_platform_init(void);
int ng_platform_poll_events(void);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_H 
