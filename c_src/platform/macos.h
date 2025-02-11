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

// New text-related functions
NGHandle ng_platform_create_text_editor(void);
NGHandle ng_platform_create_text_view(int is_editable);
int ng_platform_set_text_content(NGHandle text_handle, const char* content);
char* ng_platform_get_text_content(NGHandle text_handle);
void ng_platform_free_text_content(char* content);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_H 