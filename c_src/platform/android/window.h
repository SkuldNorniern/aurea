#ifndef NATIVE_GUI_ANDROID_WINDOW_H
#define NATIVE_GUI_ANDROID_WINDOW_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*ScaleFactorCallback)(void*, float);

NGHandle ng_android_create_window_impl(const char* title, int width, int height);
void ng_android_destroy_window_impl(NGHandle handle);
int ng_android_set_window_content(NGHandle window, NGHandle content);
float ng_android_get_scale_factor_impl(NGHandle window);
void ng_android_window_set_scale_factor_callback_impl(NGHandle window, ScaleFactorCallback callback);
void ng_android_window_set_lifecycle_callback_impl(NGHandle window);
void ng_android_window_set_title(NGHandle window, const char* title);
void ng_android_window_set_size(NGHandle window, int width, int height);
void ng_android_window_get_size(NGHandle window, int* width, int* height);
void ng_android_window_request_close(NGHandle window);
int ng_android_window_is_focused(NGHandle window);

// Helper functions for JNI integration
void ng_android_set_jni_env(JavaVM* jvm, jobject activity);
void ng_android_set_main_window_handle(void* handle);
void ng_android_set_scale_factor_callback_global(ScaleFactorCallback callback);
void ng_android_set_lifecycle_callback_enabled(int enabled);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_ANDROID_WINDOW_H

