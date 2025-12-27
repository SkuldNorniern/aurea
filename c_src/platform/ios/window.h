#ifndef NATIVE_GUI_IOS_WINDOW_H
#define NATIVE_GUI_IOS_WINDOW_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_ios_create_window_impl(const char* title, int width, int height);
void ng_ios_destroy_window_impl(NGHandle handle);
int ng_ios_set_window_content(NGHandle window, NGHandle content);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_IOS_WINDOW_H

