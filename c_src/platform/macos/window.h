#ifndef NATIVE_GUI_MACOS_WINDOW_H
#define NATIVE_GUI_MACOS_WINDOW_H

#include "../../common/types.h"

#ifdef __OBJC__
@class NSWindow;
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*ScaleFactorCallback)(void*, float);

NGHandle ng_macos_create_window(const char* title, int width, int height);
NGHandle ng_macos_create_window_with_type(const char* title, int width, int height, int window_type);
void ng_macos_destroy_window(NGHandle handle);
int ng_macos_set_window_content(NGHandle window, NGHandle content);
float ng_macos_get_scale_factor(NGHandle window);
void ng_macos_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);
void ng_macos_window_set_lifecycle_callback(NGHandle window);
void ng_macos_window_set_title(NGHandle window, const char* title);
void ng_macos_window_set_size(NGHandle window, int width, int height);
void ng_macos_window_get_size(NGHandle window, int* width, int* height);
void ng_macos_window_set_position(NGHandle window, int x, int y);
void ng_macos_window_get_position(NGHandle window, int* x, int* y);
void ng_macos_window_request_close(NGHandle window);
int ng_macos_window_is_focused(NGHandle window);
void ng_macos_window_show(NGHandle window);
void ng_macos_window_hide(NGHandle window);
int ng_macos_window_is_visible(NGHandle window);
NGHandle ng_macos_window_get_content_view(NGHandle window);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_WINDOW_H
