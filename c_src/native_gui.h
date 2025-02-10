#ifndef NATIVE_GUI_H
#define NATIVE_GUI_H

#include "common/types.h"
#include "common/errors.h"

#ifdef __cplusplus
extern "C" {
#endif

// Initialize the native GUI system
int ng_init(void);

// Cleanup the native GUI system
void ng_cleanup(void);

// Core platform-agnostic functions
NGHandle ng_create_window(const char* title, int width, int height);
void ng_destroy_window(NGHandle handle);
NGMenuHandle ng_create_menu_handle(void);
void ng_destroy_menu_handle(NGMenuHandle handle);
int ng_attach_menu_to_window(NGHandle window, NGMenuHandle menu);
int ng_add_raw_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_handle_menu_event(NGMenuHandle menu, unsigned int id);

// Platform-specific functions (implemented in platform/*.c)
int ng_platform_init(void);
void ng_platform_cleanup(void);
NGHandle ng_platform_create_window(const char* title, int width, int height);
void ng_platform_destroy_window(NGHandle handle);
NGMenuHandle ng_platform_create_menu(void);
void ng_platform_destroy_menu(NGMenuHandle handle);
int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_platform_handle_menu_event(NGMenuHandle menu, unsigned int id);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_H 