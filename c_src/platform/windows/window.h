#ifndef NATIVE_GUI_WINDOWS_WINDOW_H
#define NATIVE_GUI_WINDOWS_WINDOW_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_windows_create_window(const char* title, int width, int height);
void ng_windows_destroy_window(NGHandle handle);
int ng_windows_set_window_content(NGHandle window, NGHandle content);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_WINDOW_H

