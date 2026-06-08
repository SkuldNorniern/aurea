#include "window.h"
#include "android.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <jni.h>
#include <android/native_window.h>
#include <android/log.h>
#include <string.h>

// Global state shared with android.c (defined in android.c)
extern void* g_mainWindowHandle;
extern ScaleFactorCallback g_scaleFactorCallback;
extern int g_lifecycleCallbackEnabled;
extern JavaVM* g_jvm;
extern jobject g_activity;


// JNI helper to get JNI environment
static JNIEnv* get_jni_env(void) {
    if (!g_jvm) return NULL;
    
    JNIEnv* env = NULL;
    int status = (*g_jvm)->GetEnv(g_jvm, (void**)&env, JNI_VERSION_1_6);
    
    if (status == JNI_EDETACHED) {
        // Thread not attached, attach it
        if ((*g_jvm)->AttachCurrentThread(g_jvm, &env, NULL) != JNI_OK) {
            return NULL;
        }
    } else if (status != JNI_OK) {
        return NULL;
    }
    
    return env;
}

NGHandle ng_android_create_window_impl(const char* title, int width, int height) {
    // On Android, the window is typically the Activity or its window
    // This function should be called after the activity is created
    // The actual window handle should be set via ng_android_set_main_window_handle()
    
    // For now, create a placeholder handle
    // In a real implementation, this would get the window from the activity
    if (!g_mainWindowHandle) {
        // Use a small allocated memory as handle identifier
        static int window_id = 1;
        g_mainWindowHandle = (void*)(intptr_t)window_id++;
    }
    
    // Set the main window handle for lifecycle callbacks
    ng_android_set_main_window_handle(g_mainWindowHandle);
    
    return g_mainWindowHandle;
}

void ng_android_destroy_window_impl(NGHandle handle) {
    if (handle == g_mainWindowHandle) {
        g_mainWindowHandle = NULL;
        g_window = NULL;
    }
}

int ng_android_set_window_content(NGHandle window_handle, NGHandle content_handle) {
    if (!window_handle || !content_handle) return NG_ERROR_INVALID_HANDLE;
    
    // On Android, this would set the content view of the activity
    // This requires JNI calls to Activity.setContentView()
    // For now, just return success
    // TODO: Implement JNI call to setContentView
    
    return NG_SUCCESS;
}

float ng_android_get_scale_factor_impl(NGHandle window) {
    if (!window || !g_activity) return 1.0f;
    
    JNIEnv* env = get_jni_env();
    if (!env) return 1.0f;
    
    // Get DisplayMetrics from the activity's Resources
    jclass activity_class = (*env)->GetObjectClass(env, g_activity);
    if (!activity_class) return 1.0f;
    
    // Get Resources
    jmethodID get_resources = (*env)->GetMethodID(env, activity_class, "getResources", "()Landroid/content/res/Resources;");
    if (!get_resources) {
        (*env)->DeleteLocalRef(env, activity_class);
        return 1.0f;
    }
    
    jobject resources = (*env)->CallObjectMethod(env, g_activity, get_resources);
    if (!resources) {
        (*env)->DeleteLocalRef(env, activity_class);
        return 1.0f;
    }
    
    // Get DisplayMetrics
    jclass resources_class = (*env)->GetObjectClass(env, resources);
    jmethodID get_display_metrics = (*env)->GetMethodID(env, resources_class, "getDisplayMetrics", "()Landroid/util/DisplayMetrics;");
    if (!get_display_metrics) {
        (*env)->DeleteLocalRef(env, resources);
        (*env)->DeleteLocalRef(env, resources_class);
        (*env)->DeleteLocalRef(env, activity_class);
        return 1.0f;
    }
    
    jobject display_metrics = (*env)->CallObjectMethod(env, resources, get_display_metrics);
    if (!display_metrics) {
        (*env)->DeleteLocalRef(env, resources);
        (*env)->DeleteLocalRef(env, resources_class);
        (*env)->DeleteLocalRef(env, activity_class);
        return 1.0f;
    }
    
    // Get densityDpi
    jclass metrics_class = (*env)->GetObjectClass(env, display_metrics);
    jfieldID density_dpi_field = (*env)->GetFieldID(env, metrics_class, "densityDpi", "I");
    if (!density_dpi_field) {
        (*env)->DeleteLocalRef(env, display_metrics);
        (*env)->DeleteLocalRef(env, metrics_class);
        (*env)->DeleteLocalRef(env, resources);
        (*env)->DeleteLocalRef(env, resources_class);
        (*env)->DeleteLocalRef(env, activity_class);
        return 1.0f;
    }
    
    jint density_dpi = (*env)->GetIntField(env, display_metrics, density_dpi_field);
    
    // Clean up local references
    (*env)->DeleteLocalRef(env, display_metrics);
    (*env)->DeleteLocalRef(env, metrics_class);
    (*env)->DeleteLocalRef(env, resources);
    (*env)->DeleteLocalRef(env, resources_class);
    (*env)->DeleteLocalRef(env, activity_class);
    
    // Convert densityDpi to scale factor (160dpi = 1.0x scale)
    return (float)density_dpi / 160.0f;
}

void ng_android_window_set_scale_factor_callback_impl(NGHandle window, ScaleFactorCallback callback) {
    if (!window) return;
    
    // Store callback globally
    ng_android_set_scale_factor_callback_global(callback);
    
    // On Android, scale factor changes are detected via Configuration changes
    // The Java/Kotlin activity should call this callback when configuration changes
    // This is typically handled in onConfigurationChanged() in the activity
}

void ng_android_window_set_lifecycle_callback_impl(NGHandle window) {
    if (!window) return;
    
    // Enable lifecycle callbacks
    ng_android_set_lifecycle_callback_enabled(1);
}

void ng_android_set_jni_env(JavaVM* jvm, jobject activity) {
    ng_android_set_activity(jvm, activity);
}

void ng_android_set_scale_factor_callback_global(ScaleFactorCallback callback) {
    g_scaleFactorCallback = callback;
}

void ng_android_set_lifecycle_callback_enabled(int enabled) {
    g_lifecycleCallbackEnabled = enabled;
}

void ng_android_window_set_title(NGHandle window, const char* title) {
    // On Android, window title is typically the activity title
    // This requires JNI call to Activity.setTitle()
    // For now, this is a placeholder
    (void)window;
    (void)title;
    // TODO: Implement JNI call to setTitle
}

void ng_android_window_set_size(NGHandle window, int width, int height) {
    // Android windows are fullscreen, size is managed by the OS
    (void)window;
    (void)width;
    (void)height;
}

void ng_android_window_get_size(NGHandle window, int* width, int* height) {
    if (!window || !width || !height || !g_activity) return;
    
    JNIEnv* env = get_jni_env();
    if (!env) return;
    
    // Get window from activity
    jclass activity_class = (*env)->GetObjectClass(env, g_activity);
    if (!activity_class) return;
    
    jmethodID get_window = (*env)->GetMethodID(env, activity_class, "getWindow", "()Landroid/view/Window;");
    if (!get_window) {
        (*env)->DeleteLocalRef(env, activity_class);
        return;
    }
    
    jobject window_obj = (*env)->CallObjectMethod(env, g_activity, get_window);
    if (!window_obj) {
        (*env)->DeleteLocalRef(env, activity_class);
        return;
    }
    
    // Get window manager and display
    jclass window_class = (*env)->GetObjectClass(env, window_obj);
    jmethodID get_window_manager = (*env)->GetMethodID(env, window_class, "getWindowManager", "()Landroid/view/WindowManager;");
    if (!get_window_manager) {
        (*env)->DeleteLocalRef(env, window_obj);
        (*env)->DeleteLocalRef(env, window_class);
        (*env)->DeleteLocalRef(env, activity_class);
        return;
    }
    
    jobject window_manager = (*env)->CallObjectMethod(env, window_obj, get_window_manager);
    if (!window_manager) {
        (*env)->DeleteLocalRef(env, window_obj);
        (*env)->DeleteLocalRef(env, window_class);
        (*env)->DeleteLocalRef(env, activity_class);
        return;
    }
    
    // Get default display
    jclass wm_class = (*env)->GetObjectClass(env, window_manager);
    jmethodID get_default_display = (*env)->GetMethodID(env, wm_class, "getDefaultDisplay", "()Landroid/view/Display;");
    if (!get_default_display) {
        (*env)->DeleteLocalRef(env, window_manager);
        (*env)->DeleteLocalRef(env, wm_class);
        (*env)->DeleteLocalRef(env, window_obj);
        (*env)->DeleteLocalRef(env, window_class);
        (*env)->DeleteLocalRef(env, activity_class);
        return;
    }
    
    jobject display = (*env)->CallObjectMethod(env, window_manager, get_default_display);
    if (!display) {
        (*env)->DeleteLocalRef(env, window_manager);
        (*env)->DeleteLocalRef(env, wm_class);
        (*env)->DeleteLocalRef(env, window_obj);
        (*env)->DeleteLocalRef(env, window_class);
        (*env)->DeleteLocalRef(env, activity_class);
        return;
    }
    
    // Get display metrics
    jclass display_class = (*env)->GetObjectClass(env, display);
    jmethodID get_metrics = (*env)->GetMethodID(env, display_class, "getMetrics", "(Landroid/util/DisplayMetrics;)V");
    if (!get_metrics) {
        (*env)->DeleteLocalRef(env, display);
        (*env)->DeleteLocalRef(env, display_class);
        (*env)->DeleteLocalRef(env, window_manager);
        (*env)->DeleteLocalRef(env, wm_class);
        (*env)->DeleteLocalRef(env, window_obj);
        (*env)->DeleteLocalRef(env, window_class);
        (*env)->DeleteLocalRef(env, activity_class);
        return;
    }
    
    // Create DisplayMetrics object
    jclass metrics_class = (*env)->FindClass(env, "android/util/DisplayMetrics");
    jmethodID metrics_init = (*env)->GetMethodID(env, metrics_class, "<init>", "()V");
    jobject metrics = (*env)->NewObject(env, metrics_class, metrics_init);
    
    (*env)->CallVoidMethod(env, display, get_metrics, metrics);
    
    // Get width and height
    jfieldID width_field = (*env)->GetFieldID(env, metrics_class, "widthPixels", "I");
    jfieldID height_field = (*env)->GetFieldID(env, metrics_class, "heightPixels", "I");
    
    if (width_field && height_field) {
        *width = (*env)->GetIntField(env, metrics, width_field);
        *height = (*env)->GetIntField(env, metrics, height_field);
    }
    
    // Clean up
    (*env)->DeleteLocalRef(env, metrics);
    (*env)->DeleteLocalRef(env, metrics_class);
    (*env)->DeleteLocalRef(env, display);
    (*env)->DeleteLocalRef(env, display_class);
    (*env)->DeleteLocalRef(env, window_manager);
    (*env)->DeleteLocalRef(env, wm_class);
    (*env)->DeleteLocalRef(env, window_obj);
    (*env)->DeleteLocalRef(env, window_class);
    (*env)->DeleteLocalRef(env, activity_class);
}

void ng_android_window_request_close(NGHandle window) {
    // On Android, this would call Activity.finish()
    // This requires JNI call
    (void)window;
    // TODO: Implement JNI call to finish()
}

int ng_android_window_is_focused(NGHandle window) {
    // On Android, check if activity has window focus
    // This requires JNI call to Activity.hasWindowFocus()
    (void)window;
    // TODO: Implement JNI call to hasWindowFocus()
    return 1; // Placeholder
}

