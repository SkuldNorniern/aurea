#include "android/android.h"
#include "android/window.h"
#include "../common/errors.h"

int ng_platform_init(void) {
    return ng_android_init();
}

void ng_platform_cleanup(void) {
    ng_android_cleanup();
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    return ng_android_create_window(title, width, height);
}

void ng_platform_destroy_window(NGHandle handle) {
    ng_android_destroy_window(handle);
}

void ng_platform_window_show(NGHandle window) {
    // Android windows are Activities, managed by OS
    (void)window;
}

void ng_platform_window_hide(NGHandle window) {
    // Android windows are Activities, managed by OS
    (void)window;
}

int ng_platform_window_is_visible(NGHandle window) {
    // Android windows are Activities, managed by OS
    (void)window;
    return 1;
}

NGMenuHandle ng_platform_create_menu(void) {
    return ng_android_create_menu();
}

void ng_platform_destroy_menu(NGMenuHandle handle) {
    ng_android_destroy_menu(handle);
}

int ng_platform_attach_menu(NGHandle _window, NGMenuHandle _menu) {
    // Android doesn't support menu bars in the same way
    (void)_window;
    (void)_menu;
    return NG_SUCCESS;
}

int ng_platform_add_menu_item(NGMenuHandle _menu, const char* _title, unsigned int _id) {
    // Android doesn't support menu bars
    (void)_menu;
    (void)_title;
    (void)_id;
    return NG_SUCCESS;
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle _parentMenu, const char* _title) {
    // Android doesn't support menu bars
    (void)_parentMenu;
    (void)_title;
    return NULL;
}

int ng_platform_run(void) {
    // Android app runs via Activity lifecycle
    // This function should not be called on Android
    return NG_SUCCESS;
}

int ng_platform_poll_events(void) {
    // Android events are handled by the Looper; manual polling is generally not done this way.
    return NG_SUCCESS;
}

int ng_platform_set_window_content(NGHandle window, NGHandle content) {
    extern int ng_android_set_window_content(NGHandle window, NGHandle content);
    return ng_android_set_window_content(window, content);
}

NGHandle ng_platform_create_button(const char* title, unsigned int id) {
    return ng_android_create_button(title);
}

NGHandle ng_platform_create_label(const char* text) {
    return ng_android_create_label(text);
}

NGHandle ng_platform_create_box(int is_vertical) {
    // TODO: Implement for Android
    (void)is_vertical;
    return NULL;
}

int ng_platform_box_add(NGHandle box, NGHandle element) {
    // TODO: Implement for Android
    (void)box;
    (void)element;
    return NG_SUCCESS;
}

NGHandle ng_platform_create_text_editor(unsigned int _id) {
    // TODO: Implement for Android
    (void)_id;
    return NULL;
}

NGHandle ng_platform_create_text_view(int _is_editable, unsigned int _id) {
    // TODO: Implement for Android
    (void)_is_editable;
    (void)_id;
    return NULL;
}

int ng_platform_set_text_content(NGHandle _text_handle, const char* _content) {
    // TODO: Implement for Android
    (void)_text_handle;
    (void)_content;
    return NG_SUCCESS;
}

char* ng_platform_get_text_content(NGHandle _text_handle) {
    // TODO: Implement for Android
    (void)_text_handle;
    return NULL;
}

void ng_platform_free_text_content(char* content) {
    if (content) {
        free(content);
    }
}

NGHandle ng_platform_create_canvas(int width, int height) {
    return ng_android_create_canvas(width, height);
}

void ng_platform_canvas_invalidate(NGHandle _canvas) {
    // TODO: Implement canvas invalidation for Android
    (void)_canvas;
}

float ng_platform_get_scale_factor(NGHandle window) {
    return ng_android_get_scale_factor(window);
}

void ng_platform_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    ng_android_window_set_scale_factor_callback(window, callback);
}

void ng_platform_window_set_lifecycle_callback(NGHandle window) {
    ng_android_window_set_lifecycle_callback(window);
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
