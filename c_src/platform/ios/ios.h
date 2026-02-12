#ifndef NATIVE_GUI_IOS_H
#define NATIVE_GUI_IOS_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*ScaleFactorCallback)(void*, float);

// iOS platform initialization
int ng_ios_init(void);
void ng_ios_cleanup(void);

// iOS window management
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

// iOS menu management (limited on iOS)
NGMenuHandle ng_ios_create_menu(void);
void ng_ios_destroy_menu(NGMenuHandle handle);

// iOS elements
NGHandle ng_ios_create_button(const char* title, unsigned int id);
NGHandle ng_ios_create_label(const char* text);
NGHandle ng_ios_create_canvas(int width, int height);

// Platform invalidation stubs (for Rust compatibility)
void ng_platform_button_invalidate(NGHandle handle);
void ng_platform_label_invalidate(NGHandle handle);
void ng_platform_canvas_invalidate(NGHandle handle);
void ng_platform_text_editor_invalidate(NGHandle handle);
void ng_platform_text_view_invalidate(NGHandle handle);
void ng_platform_progress_bar_invalidate(NGHandle handle);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_IOS_H

