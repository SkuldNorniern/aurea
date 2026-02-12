#ifndef NATIVE_GUI_LINUX_WINDOW_H
#define NATIVE_GUI_LINUX_WINDOW_H

#include "common/types.h"
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_linux_create_window(const char* title, int width, int height);
NGHandle ng_linux_create_window_with_type(const char* title, int width, int height, int window_type);
void ng_linux_destroy_window(NGHandle handle);
void ng_linux_window_show(NGHandle window);
void ng_linux_window_hide(NGHandle window);
int ng_linux_window_is_visible(NGHandle window);
int ng_linux_set_window_content(NGHandle window, NGHandle content);
float ng_linux_get_scale_factor(NGHandle window);
typedef void (*ScaleFactorCallback)(void*, float);
void ng_linux_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);
void ng_linux_window_set_lifecycle_callback(NGHandle window);
void ng_linux_window_set_title(NGHandle window, const char* title);
void ng_linux_window_set_size(NGHandle window, int width, int height);
void ng_linux_window_get_size(NGHandle window, int* width, int* height);
void ng_linux_window_set_position(NGHandle window, int x, int y);
void ng_linux_window_get_position(NGHandle window, int* x, int* y);
void ng_linux_window_request_close(NGHandle window);
int ng_linux_window_is_focused(NGHandle window);
int ng_linux_window_get_xcb_handle(NGHandle window, uint32_t* xcb_window, void** xcb_connection);
int ng_linux_window_get_wayland_handle(NGHandle window, void** surface, void** display);
int ng_linux_window_set_cursor_visible(NGHandle window, int visible);
int ng_linux_window_set_cursor_grab(NGHandle window, int mode);

// Internal function to get main vbox (used by menu.c)
// Note: Returns GtkWidget* but declared as void* to avoid GTK dependency in header
void* ng_linux_get_main_vbox(void);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_LINUX_WINDOW_H
