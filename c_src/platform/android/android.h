#ifndef NATIVE_GUI_ANDROID_H
#define NATIVE_GUI_ANDROID_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*ScaleFactorCallback)(void*, float);

// Android platform initialization
int ng_android_init(void);
void ng_android_cleanup(void);

// Android activity/window management
NGHandle ng_android_create_window(const char* title, int width, int height);
void ng_android_destroy_window(NGHandle handle);
float ng_android_get_scale_factor(NGHandle window);
void ng_android_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);
void ng_android_window_set_lifecycle_callback(NGHandle window);

// JNI integration functions
void ng_android_set_activity(JavaVM* jvm, jobject activity);

// Android lifecycle callbacks (called from Java/Kotlin Activity)
void ng_android_on_pause(void);
void ng_android_on_resume(void);
void ng_android_on_destroy(void);
void ng_android_on_memory_warning(void);
void ng_android_on_surface_lost(void);
void ng_android_on_surface_recreated(void);

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

