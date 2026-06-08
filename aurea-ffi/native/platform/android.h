#ifndef NATIVE_GUI_ANDROID_PLATFORM_H
#define NATIVE_GUI_ANDROID_PLATFORM_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

int ng_platform_init(void);
void ng_platform_cleanup(void);
NGHandle ng_platform_create_window(const char* title, int width, int height);
void ng_platform_destroy_window(NGHandle handle);

#ifdef __cplusplus
}
#endif

#endif
