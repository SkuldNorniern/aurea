#ifndef NATIVE_GUI_MACOS_H
#define NATIVE_GUI_MACOS_H

#include "../common/types.h"

#ifdef __OBJC__
@class NSWindow;
@class NSMenu;
@class NSView;
@class NSButton;
@class NSTextField;
@class NSStackView;
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

// New element-related functions
NGHandle ng_platform_create_button(const char* title);
NGHandle ng_platform_create_label(const char* text);
NGHandle ng_platform_create_box(int is_vertical);
int ng_platform_box_add(NGHandle box, NGHandle element);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_H 