#ifndef NATIVE_GUI_MACOS_WINDOW_H
#define NATIVE_GUI_MACOS_WINDOW_H

#include "../../common/types.h"

#ifdef __OBJC__
@class NSWindow;
#endif

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_macos_create_window(const char* title, int width, int height);
void ng_macos_destroy_window(NGHandle handle);
int ng_macos_set_window_content(NGHandle window, NGHandle content);
float ng_macos_get_scale_factor(NGHandle window);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_WINDOW_H 