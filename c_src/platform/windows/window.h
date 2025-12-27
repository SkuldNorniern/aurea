#ifndef NATIVE_GUI_WINDOWS_WINDOW_H
#define NATIVE_GUI_WINDOWS_WINDOW_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*ScaleFactorCallback)(void*, float);

NGHandle ng_windows_create_window(const char* title, int width, int height);
void ng_windows_destroy_window(NGHandle handle);
int ng_windows_set_window_content(NGHandle window, NGHandle content);
float ng_windows_get_scale_factor(NGHandle window);
void ng_windows_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_WINDOW_H

