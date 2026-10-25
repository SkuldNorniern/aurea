#ifndef NATIVE_GUI_LINUX_WINDOW_H
#define NATIVE_GUI_LINUX_WINDOW_H

#include "common/platform_api.h"
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_linux_create_window(const char* title, int width, int height);
NGHandle ng_linux_create_window_with_type(const char* title, int width, int height, int window_type);
void ng_linux_destroy_window(NGHandle handle);
void ng_linux_destroy_element(NGHandle element);
int ng_linux_detach_element(NGHandle element);
void ng_linux_element_retain(NGHandle element);
void ng_linux_element_release(NGHandle element);
void ng_linux_window_show(NGHandle window);
void ng_linux_window_hide(NGHandle window);
void ng_linux_window_set_close_guard(NGHandle window, int guarded);
int ng_linux_window_is_visible(NGHandle window);
int ng_linux_window_occlusion(NGHandle window);
int ng_linux_set_window_content(NGHandle window, NGHandle content);
float ng_linux_get_scale_factor(NGHandle window);
void ng_linux_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);
void ng_linux_window_set_lifecycle_callback(NGHandle window);
void ng_linux_window_set_title(NGHandle window, const char* title);
int ng_linux_window_set_icon_rgba(
    NGHandle window,
    const unsigned char* rgba,
    unsigned int width,
    unsigned int height
);
void ng_linux_window_set_size(NGHandle window, int width, int height);
int ng_linux_window_set_min_size(NGHandle window, int width, int height);
void ng_linux_window_get_size(NGHandle window, int* width, int* height);
void ng_linux_window_set_position(NGHandle window, int x, int y);
void ng_linux_window_get_position(NGHandle window, int* x, int* y);
void ng_linux_window_request_close(NGHandle window);
int ng_linux_window_is_focused(NGHandle window);
int ng_linux_window_get_xcb_handle(NGHandle window, uint32_t* xcb_window, void** xcb_connection);
int ng_linux_window_get_wayland_handle(NGHandle window, void** surface, void** display);
int ng_linux_window_set_cursor_visible(NGHandle window, int visible);
int ng_linux_window_set_cursor_grab(NGHandle window, int mode);
int ng_linux_window_set_cursor_icon(NGHandle window, int icon);
int ng_linux_window_set_cursor_rgba(NGHandle window, const unsigned char* rgba, unsigned int width, unsigned int height, unsigned int hot_x, unsigned int hot_y);
NGHandle ng_linux_window_get_content_view(NGHandle window);

char* ng_linux_get_clipboard_text(void);
void ng_linux_free_clipboard_text(char* text);
int ng_linux_set_clipboard_text(const char* text);

#ifdef __cplusplus
}
#endif

#endif
