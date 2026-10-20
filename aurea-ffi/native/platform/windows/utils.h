#ifndef NATIVE_GUI_WINDOWS_UTILS_H
#define NATIVE_GUI_WINDOWS_UTILS_H

#include <windows.h>
#include "common/platform_api.h"

#ifdef __cplusplus
extern "C" {
#endif

int ng_windows_init(void);
void ng_windows_cleanup(void);
int ng_windows_run(void);
int ng_windows_poll_events(void);
int ng_windows_wait_events(int timeout_ms);
void ng_windows_request_frame(void);
#define AUREA_FRAME_TIMER_ID 0xA0E2
void ng_windows_modal_begin(HWND hwnd);
void ng_windows_modal_end(HWND hwnd);
void ng_windows_modal_tick(void);
void ng_windows_display_changed(void);
double ng_windows_logical(HWND hwnd, int physical);
void ng_windows_fit_content(HWND window);
unsigned int ng_windows_window_refresh_hz(void);
void ng_windows_present(void);
BOOL ng_windows_is_initialized(void);
const wchar_t* ng_windows_get_class_name(void);
wchar_t* ng_windows_utf8_to_wide(const char* utf8);
/* `name` from `module`, loading it if needed; NULL on systems without it. */
FARPROC ng_windows_api(const char* module, const char* name);
/* The activation context for common controls 6, from shell32; INVALID_HANDLE_VALUE when missing. */
HANDLE ng_windows_common_controls_v6(void);
char* ng_windows_wide_to_utf8(const wchar_t* wide, int len);
char* ng_windows_window_text(HWND hwnd);
int ng_windows_set_window_text(HWND hwnd, const char* utf8);
void ng_windows_register_scale_callback(HWND hwnd, ScaleFactorCallback callback);
int ng_windows_window_set_shortcuts(NGHandle window, const unsigned int* keycodes, const unsigned int* modifiers, int count);
int ng_windows_take_shortcut(HWND root, WPARAM vk);

unsigned int ng_windows_modifiers(void);

int ng_windows_reduced_motion(void);
int ng_windows_locales(char* buf, int cap);

#ifdef __cplusplus
}
#endif

#endif

