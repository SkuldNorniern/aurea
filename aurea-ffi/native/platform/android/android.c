#include "android.h"
#include "window.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <jni.h>
#include <android/native_window.h>
#include <android/log.h>
#include <pthread.h>

void* g_mainWindowHandle = NULL;
ScaleFactorCallback g_scaleFactorCallback = NULL;
int g_lifecycleCallbackEnabled = 0;
JavaVM* g_jvm = NULL;
jobject g_activity = NULL;

static pthread_key_t g_attached_key;
static pthread_once_t g_attached_once = PTHREAD_ONCE_INIT;

static void ng_android_detach_thread(void* value) {
    (void)value;
    if (g_jvm) (*g_jvm)->DetachCurrentThread(g_jvm);
}

static void ng_android_make_attached_key(void) {
    pthread_key_create(&g_attached_key, ng_android_detach_thread);
}

JNIEnv* ng_android_jni_env(void) {
    if (!g_jvm) return NULL;
    JNIEnv* env = NULL;
    int status = (*g_jvm)->GetEnv(g_jvm, (void**)&env, JNI_VERSION_1_6);
    if (status == JNI_OK) return env;
    if (status != JNI_EDETACHED) return NULL;
    if ((*g_jvm)->AttachCurrentThread(g_jvm, &env, NULL) != JNI_OK) return NULL;
    pthread_once(&g_attached_once, ng_android_make_attached_key);
    pthread_setspecific(g_attached_key, (void*)1);
    return env;
}

static void ng_android_release_activity(void) {
    if (!g_activity) return;
    JNIEnv* env = ng_android_jni_env();
    if (env) (*env)->DeleteGlobalRef(env, g_activity);
    g_activity = NULL;
}

int ng_android_init(void) {
    return NG_SUCCESS;
}

void ng_android_cleanup(void) {
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
    extern float ng_android_get_scale_factor_impl(NGHandle window);
    return ng_android_get_scale_factor_impl(window);
}

void ng_android_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    extern void ng_android_window_set_scale_factor_callback_impl(NGHandle window, ScaleFactorCallback callback);
    ng_android_window_set_scale_factor_callback_impl(window, callback);
}

void ng_android_window_set_lifecycle_callback(NGHandle window) {
    extern void ng_android_window_set_lifecycle_callback_impl(NGHandle window);
    ng_android_window_set_lifecycle_callback_impl(window);
}

void ng_android_on_pause(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 2);
    }
}

void ng_android_on_resume(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 3);
    }
}

void ng_android_on_destroy(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 4);
    }
    ng_android_release_activity();
}

void ng_android_on_memory_warning(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 8);
    }
}

void ng_android_on_surface_lost(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 9);
    }
}

void ng_android_on_surface_recreated(void) {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 10);
    }
}

void ng_android_set_activity(JavaVM* jvm, jobject activity) {
    g_jvm = jvm;
    ng_android_release_activity();
    if (activity) {
        JNIEnv* env = ng_android_jni_env();
        if (env) {
            g_activity = (*env)->NewGlobalRef(env, activity);
        }
    }
}

NGMenuHandle ng_android_create_menu(void) {
    return NULL;
}

void ng_android_destroy_menu(NGMenuHandle handle) {
    (void)handle;
}

NGHandle ng_android_create_button(const char* title) {
    (void)title;
    return NULL;
}

NGHandle ng_android_create_label(const char* text) {
    (void)text;
    return NULL;
}

NGHandle ng_android_create_canvas(int width, int height) {
    (void)width;
    (void)height;
    return NULL;
}

