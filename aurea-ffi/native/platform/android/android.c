#include "android.h"
#include "window.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <jni.h>
#include <android/native_window.h>
#include <android/log.h>

// Global state for Android lifecycle and scale factor (shared with window.c)
void* g_mainWindowHandle = NULL;
ScaleFactorCallback g_scaleFactorCallback = NULL;
int g_lifecycleCallbackEnabled = 0;
JavaVM* g_jvm = NULL;
jobject g_activity = NULL;

int ng_android_init(void) {
    // Android initialization is handled by JNI
    // This is called from the native activity or main activity
    return NG_SUCCESS;
}

void ng_android_cleanup(void) {
    // Android cleanup is handled by activity lifecycle
    g_mainWindowHandle = NULL;
    g_scaleFactorCallback = NULL;
    g_lifecycleCallbackEnabled = 0;
}

NGHandle ng_android_create_window(const char* title, int width, int height) {
    return ng_android_create_window_impl(title, width, height);
}

void ng_android_destroy_window(NGHandle handle) {
    ng_android_destroy_window_impl(handle);
}

float ng_android_get_scale_factor(NGHandle window) {
    // Forward to window.c implementation
    extern float ng_android_get_scale_factor_impl(NGHandle window);
    return ng_android_get_scale_factor_impl(window);
}

void ng_android_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    // Forward to window.c implementation
    extern void ng_android_window_set_scale_factor_callback_impl(NGHandle window, ScaleFactorCallback callback);
    ng_android_window_set_scale_factor_callback_impl(window, callback);
}

void ng_android_window_set_lifecycle_callback(NGHandle window) {
    // Forward to window.c implementation
    extern void ng_android_window_set_lifecycle_callback_impl(NGHandle window);
    ng_android_window_set_lifecycle_callback_impl(window);
}

void ng_platform_window_set_title(NGHandle window, const char* title) {
    extern void ng_android_window_set_title(NGHandle window, const char* title);
    ng_android_window_set_title(window, title);
}

void ng_platform_window_set_size(NGHandle window, int width, int height) {
    extern void ng_android_window_set_size(NGHandle window, int width, int height);
    ng_android_window_set_size(window, width, height);
}

void ng_platform_window_get_size(NGHandle window, int* width, int* height) {
    extern void ng_android_window_get_size(NGHandle window, int* width, int* height);
    ng_android_window_get_size(window, width, height);
}

void ng_platform_window_request_close(NGHandle window) {
    extern void ng_android_window_request_close(NGHandle window);
    ng_android_window_request_close(window);
}

int ng_platform_window_is_focused(NGHandle window) {
    extern int ng_android_window_is_focused(NGHandle window);
    return ng_android_window_is_focused(window);
}

// Helper functions are now in window.c to avoid circular dependencies

// Android lifecycle callbacks (called from Java/Kotlin via JNI)
// These should be called from the Activity lifecycle methods
void ng_android_on_pause(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 2); // ApplicationPaused = 2
    }
}

void ng_android_on_resume(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 3); // ApplicationResumed = 3
    }
}

void ng_android_on_destroy(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 4); // ApplicationDestroyed = 4
    }
}

void ng_android_on_memory_warning(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 8); // MemoryWarning = 8
    }
}

void ng_android_on_surface_lost(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 9); // SurfaceLost = 9
    }
}

void ng_android_on_surface_recreated(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 10); // SurfaceRecreated = 10
    }
}

// JNI function to set the activity and JVM
// This should be called from JNI_OnLoad or from the activity's onCreate
void ng_android_set_activity(JavaVM* jvm, jobject activity) {
    g_jvm = jvm;
    if (activity) {
        JNIEnv* env = NULL;
        int status = (*g_jvm)->GetEnv(g_jvm, (void**)&env, JNI_VERSION_1_6);
        if (status == JNI_EDETACHED) {
            (*g_jvm)->AttachCurrentThread(g_jvm, &env, NULL);
        }
        if (env) {
            g_activity = (*env)->NewGlobalRef(env, activity);
        }
    }
}

NGMenuHandle ng_android_create_menu(void) {
    // TODO: Create Android menu
    return NULL;
}

void ng_android_destroy_menu(NGMenuHandle handle) {
    // TODO: Destroy Android menu
}

NGHandle ng_android_create_button(const char* title) {
    // TODO: Create Android button view
    return NULL;
}

NGHandle ng_android_create_label(const char* text) {
    // TODO: Create Android text view
    return NULL;
}

NGHandle ng_android_create_canvas(int width, int height) {
    // TODO: Create Android canvas/view for rendering (SurfaceView, GLSurfaceView, etc.)
    return NULL;
}

