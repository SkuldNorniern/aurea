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
BOOL ng_windows_is_initialized(void);
const char* ng_windows_get_class_name(void);
void ng_windows_register_scale_callback(HWND hwnd, ScaleFactorCallback callback);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_UTILS_H

