#ifndef NATIVE_GUI_IOS_H
#define NATIVE_GUI_IOS_H

#include "common/platform_api.h"

#ifdef __cplusplus
extern "C" {
#endif

int ng_ios_init(void);
void ng_ios_cleanup(void);

NGHandle ng_ios_create_window(const char* title, int width, int height);
void ng_ios_destroy_window(NGHandle handle);
int ng_ios_set_window_content(NGHandle window, NGHandle content);
float ng_ios_get_scale_factor(NGHandle window);
void ng_ios_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);
void ng_ios_window_set_lifecycle_callback(NGHandle window);
void ng_ios_window_set_title(NGHandle window, const char* title);
void ng_ios_window_set_size(NGHandle window, int width, int height);
void ng_ios_window_get_size(NGHandle window, int* width, int* height);
void ng_ios_window_request_close(NGHandle window);
int ng_ios_window_is_focused(NGHandle window);

NGMenuHandle ng_ios_create_menu(void);
void ng_ios_destroy_menu(NGMenuHandle handle);
int ng_ios_attach_menu(NGMenuHandle menu);
int ng_ios_add_menu_item(NGMenuHandle menu, const char* title, unsigned int item);
int ng_ios_add_menu_check_item(NGMenuHandle menu, const char* title, unsigned int item, int checked);
int ng_ios_add_menu_separator(NGMenuHandle menu);
NGMenuHandle ng_ios_create_submenu(NGMenuHandle parent, const char* title);
int ng_ios_menu_clear(NGMenuHandle menu);
int ng_ios_menu_item_set_enabled(NGMenuHandle menu, unsigned int item, int enabled);
int ng_ios_menu_item_set_checked(NGMenuHandle menu, unsigned int item, int checked);
int ng_ios_menu_item_set_title(NGMenuHandle menu, unsigned int item, const char* title);
int ng_ios_menu_item_remove(NGMenuHandle menu, unsigned int item);

#ifdef __OBJC__
#import <UIKit/UIKit.h>

/* The key commands for the menu's shortcuts. */
NSArray<UIKeyCommand*>* ng_ios_key_commands(void);
/* Runs the menu item a key command stands for, when it is enabled. */
void ng_ios_key_command(UIKeyCommand* key);
/* Puts the app's menus into the menu bar. */
void ng_ios_build_menu(id<UIMenuBuilder> builder);
#endif

NGHandle ng_ios_create_button(const char* title, unsigned int id);
NGHandle ng_ios_create_label(const char* text);
NGHandle ng_ios_create_canvas(int width, int height);

void ng_platform_button_invalidate(NGHandle handle);
void ng_platform_label_invalidate(NGHandle handle);
int ng_platform_label_set_text(NGHandle handle, const char* text);
int ng_platform_label_set_style(NGHandle handle, int style);
NGHandle ng_platform_create_scroll_view(void);
int ng_platform_scroll_view_set_content(NGHandle scroll, NGHandle element);
void ng_platform_canvas_invalidate(NGHandle handle);
void ng_platform_text_editor_invalidate(NGHandle handle);
void ng_platform_text_view_invalidate(NGHandle handle);
void ng_platform_progress_bar_invalidate(NGHandle handle);

#ifdef __cplusplus
}
#endif

#endif

