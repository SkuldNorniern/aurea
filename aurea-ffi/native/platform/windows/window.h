#ifndef NATIVE_GUI_WINDOWS_WINDOW_H
#define NATIVE_GUI_WINDOWS_WINDOW_H

#include "common/platform_api.h"
#include <windows.h>

/* window property set while closing the window only hides it. */
#define AUREA_HIDE_ON_CLOSE_PROPERTY "AureaHideOnClose"
#define AUREA_CLOSE_GUARD_PROPERTY "AureaCloseGuard"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_windows_create_window(const char* title, int width, int height);
NGHandle ng_windows_create_window_with_type(const char* title, int width, int height, int window_type);
void ng_windows_destroy_window(NGHandle handle);
void ng_windows_destroy_element(NGHandle element);
int ng_windows_detach_element(NGHandle element);
void ng_windows_window_show(NGHandle window);
void ng_windows_window_hide(NGHandle window);
int ng_windows_window_is_visible(NGHandle window);
int ng_windows_set_window_content(NGHandle window, NGHandle content);
float ng_windows_get_scale_factor(NGHandle window);
void ng_windows_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);
void ng_windows_window_set_lifecycle_callback(NGHandle window);
void ng_windows_window_set_title(NGHandle window, const char* title);
int ng_windows_window_set_icon_rgba(
    NGHandle window,
    const unsigned char* rgba,
    unsigned int width,
    unsigned int height
);
HICON ng_windows_icon_from_rgba(const unsigned char* rgba, unsigned int width, unsigned int height);
int ng_windows_window_set_hide_on_close(NGHandle window, int hide);
void ng_windows_window_set_close_guard(NGHandle window, int guarded);
void ng_windows_window_set_size(NGHandle window, int width, int height);
int ng_windows_window_set_min_size(NGHandle window, int width, int height);
/* Logical minimum client size plus 1, so 0 still means unset. */
#define AUREA_MIN_CLIENT_W_PROP "AureaMinClientW"
#define AUREA_MIN_CLIENT_H_PROP "AureaMinClientH"
void ng_windows_window_get_size(NGHandle window, int* width, int* height);
void ng_windows_window_set_position(NGHandle window, int x, int y);
void ng_windows_window_get_position(NGHandle window, int* x, int* y);
void ng_windows_window_request_close(NGHandle window);
int ng_windows_window_is_focused(NGHandle window);
int ng_windows_window_set_cursor_visible(NGHandle window, int visible);
int ng_windows_window_set_cursor_grab(NGHandle window, int mode);
int ng_windows_window_set_cursor_icon(NGHandle window, int icon);
int ng_windows_window_set_cursor_rgba(NGHandle window, const unsigned char* rgba, unsigned int width, unsigned int height, unsigned int hot_x, unsigned int hot_y);
void ng_windows_cursor_grab_activate(void* hwnd, int active);
void ng_windows_cursor_grab_refresh(void* hwnd);
void ng_windows_release_window_input(void* hwnd);
void* ng_windows_window_cursor(NGHandle window);
NGHandle ng_windows_window_get_content_view(NGHandle window);

char* ng_windows_get_clipboard_text(void);
void ng_windows_free_clipboard_text(char* text);
int ng_windows_set_clipboard_text(const char* text);

#ifdef __cplusplus
}
#endif

#endif
