#ifndef NATIVE_GUI_WINDOWS_UTILS_H
#define NATIVE_GUI_WINDOWS_UTILS_H

#include <windows.h>

#ifdef __cplusplus
extern "C" {
#endif

// Utility functions for Windows platform

int ng_windows_init(void);
void ng_windows_cleanup(void);
BOOL ng_windows_is_initialized(void);
const char* ng_windows_get_class_name(void);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_UTILS_H

