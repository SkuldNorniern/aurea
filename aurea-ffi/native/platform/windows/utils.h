#ifndef NATIVE_GUI_WINDOWS_UTILS_H
#define NATIVE_GUI_WINDOWS_UTILS_H

#include <windows.h>
#include "common/platform_api.h"

#ifdef __cplusplus
extern "C" {
#endif


// Utility functions for Windows platform

int ng_windows_init(void);
void ng_windows_cleanup(void);
int ng_windows_run(void);
int ng_windows_poll_events(void);
void ng_windows_request_frame(void);
#define AUREA_FRAME_TIMER_ID 0xA0E2
void ng_windows_modal_begin(HWND hwnd);
void ng_windows_modal_end(HWND hwnd);
void ng_windows_modal_tick(void);
void ng_windows_display_changed(void);
unsigned int ng_windows_window_refresh_hz(void);
void ng_windows_present(void);
BOOL ng_windows_is_initialized(void);
const wchar_t* ng_windows_get_class_name(void);
/* UTF-8 to UTF-16 on the heap; the caller frees it. */
wchar_t* ng_windows_utf8_to_wide(const char* utf8);
void ng_windows_register_scale_callback(HWND hwnd, ScaleFactorCallback callback);
int ng_windows_window_set_shortcuts(NGHandle window, const unsigned int* keycodes, const unsigned int* modifiers, int count);
/* 1 if a key going to a child of root was root's shortcut and is now handled. */
int ng_windows_take_shortcut(HWND root, WPARAM vk);

int ng_windows_reduced_motion(void);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_UTILS_H

