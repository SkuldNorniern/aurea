#ifndef NATIVE_GUI_ANDROID_H
#define NATIVE_GUI_ANDROID_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

// Android platform initialization
int ng_android_init(void);
void ng_android_cleanup(void);

// Android activity/window management
NGHandle ng_android_create_window(const char* title, int width, int height);
void ng_android_destroy_window(NGHandle handle);

// Android menu management
NGMenuHandle ng_android_create_menu(void);
void ng_android_destroy_menu(NGMenuHandle handle);

// Android elements
NGHandle ng_android_create_button(const char* title);
NGHandle ng_android_create_label(const char* text);
NGHandle ng_android_create_canvas(int width, int height);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_ANDROID_H

