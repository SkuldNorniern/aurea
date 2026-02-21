#ifndef NATIVE_GUI_WINDOWS_WINDOW_H
#define NATIVE_GUI_WINDOWS_WINDOW_H

#include "common/platform_api.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_windows_create_window(const char* title, int width, int height);
NGHandle ng_windows_create_window_with_type(const char* title, int width, int height, int window_type);
void ng_windows_destroy_window(NGHandle handle);
void ng_windows_window_show(NGHandle window);
void ng_windows_window_hide(NGHandle window);
int ng_windows_window_is_visible(NGHandle window);
int ng_windows_set_window_content(NGHandle window, NGHandle content);
float ng_windows_get_scale_factor(NGHandle window);
void ng_windows_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);
void ng_windows_window_set_lifecycle_callback(NGHandle window);
void ng_windows_window_set_title(NGHandle window, const char* title);
void ng_windows_window_set_size(NGHandle window, int width, int height);
void ng_windows_window_get_size(NGHandle window, int* width, int* height);
void ng_windows_window_set_position(NGHandle window, int x, int y);
void ng_windows_window_get_position(NGHandle window, int* x, int* y);
void ng_windows_window_request_close(NGHandle window);
int ng_windows_window_is_focused(NGHandle window);
int ng_windows_window_set_cursor_visible(NGHandle window, int visible);
int ng_windows_window_set_cursor_grab(NGHandle window, int mode);
NGHandle ng_windows_window_get_content_view(NGHandle window);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_WINDOW_H
