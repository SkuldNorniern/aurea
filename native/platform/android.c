#include "android/android.h"
#include "android/window.h"
#include "common/errors.h"
#include "common/platform_api.h"
#include <stdlib.h>

int ng_platform_init(void) {
    return ng_android_init();
}

void ng_platform_cleanup(void) {
    ng_android_cleanup();
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    return ng_android_create_window(title, width, height);
}

NGHandle ng_platform_create_window_with_type(const char* title, int width, int height, int window_type) {
    (void)window_type;
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

void ng_platform_button_invalidate(NGHandle _button) {
    (void)_button;
}

void ng_platform_label_invalidate(NGHandle _label) {
    (void)_label;
}

void ng_platform_box_invalidate(NGHandle _box) {
    (void)_box;
}

NGHandle ng_platform_create_split_view(int is_vertical) {
    (void)is_vertical;
    return NULL;
}

int ng_platform_split_view_add(NGHandle _split, NGHandle _element) {
    (void)_split;
    (void)_element;
    return NG_SUCCESS;
}

int ng_platform_split_view_set_divider_position(NGHandle _split, int _index, float _pos) {
    (void)_split;
    (void)_index;
    (void)_pos;
    return NG_SUCCESS;
}

void ng_platform_text_editor_invalidate(NGHandle _h) {
    (void)_h;
}

void ng_platform_text_view_invalidate(NGHandle _h) {
    (void)_h;
}

NGHandle ng_platform_create_text_field(void) {
    return NULL;
}

void ng_platform_canvas_invalidate_rect(NGHandle _canvas, float _x, float _y, float _w, float _h) {
    (void)_canvas;
    (void)_x;
    (void)_y;
    (void)_w;
    (void)_h;
}

void ng_platform_canvas_update_buffer(NGHandle _canvas, const unsigned char* _buf, unsigned int _size, unsigned int _w, unsigned int _h) {
    (void)_canvas;
    (void)_buf;
    (void)_size;
    (void)_w;
    (void)_h;
}

void ng_platform_canvas_get_size(NGHandle _canvas, unsigned int* width, unsigned int* height) {
    (void)_canvas;
    if (width) *width = 0;
    if (height) *height = 0;
}

NGHandle ng_platform_canvas_get_window(NGHandle _canvas) {
    (void)_canvas;
    return NULL;
}

NGHandle ng_platform_canvas_get_native_handle(NGHandle _canvas) {
    (void)_canvas;
    return NULL;
}

int ng_platform_canvas_get_xcb_handle(NGHandle _canvas, uint32_t* _xcb_window, void** _xcb_connection) {
    (void)_canvas;
    (void)_xcb_window;
    (void)_xcb_connection;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_platform_canvas_get_wayland_handle(NGHandle _canvas, void** _surface, void** _display) {
    (void)_canvas;
    (void)_surface;
    (void)_display;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

NGHandle ng_platform_window_get_content_view(NGHandle _window) {
    (void)_window;
    return NULL;
}

void ng_platform_window_set_position(NGHandle _window, int _x, int _y) {
    (void)_window;
    (void)_x;
    (void)_y;
}

void ng_platform_window_get_position(NGHandle _window, int* _x, int* _y) {
    (void)_window;
    if (_x) *_x = 0;
    if (_y) *_y = 0;
}

int ng_platform_window_get_xcb_handle(NGHandle _window, uint32_t* _xcb_window, void** _xcb_connection) {
    (void)_window;
    (void)_xcb_window;
    (void)_xcb_connection;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_platform_window_get_wayland_handle(NGHandle _window, void** _surface, void** _display) {
    (void)_window;
    (void)_surface;
    (void)_display;
    return NG_ERROR_PLATFORM_SPECIFIC;
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

int ng_platform_add_menu_separator(NGMenuHandle _menu) {
    (void)_menu;
    return NG_SUCCESS;
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle _parentMenu, const char* _title) {
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

int ng_platform_box_add(NGHandle box, NGHandle element, float weight) {
    // TODO: Implement for Android
    (void)box;
    (void)element;
    (void)weight;
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

int ng_platform_window_set_cursor_visible(NGHandle window, int visible) {
    (void)window;
    (void)visible;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_platform_window_set_cursor_grab(NGHandle window, int mode) {
    (void)window;
    (void)mode;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

NGHandle ng_platform_create_image_view(void) {
    return NULL;
}

int ng_platform_image_view_load_from_path(NGHandle _v, const char* _path) {
    (void)_v;
    (void)_path;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_platform_image_view_load_from_data(NGHandle _v, const unsigned char* _d, unsigned int _s) {
    (void)_v;
    (void)_d;
    (void)_s;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

void ng_platform_image_view_set_scaling(NGHandle _v, int _mode) {
    (void)_v;
    (void)_mode;
}

void ng_platform_image_view_invalidate(NGHandle _v) {
    (void)_v;
}

NGHandle ng_platform_create_slider(double _min, double _max) {
    (void)_min;
    (void)_max;
    return NULL;
}

int ng_platform_slider_set_value(NGHandle _s, double _v) {
    (void)_s;
    (void)_v;
    return NG_SUCCESS;
}

double ng_platform_slider_get_value(NGHandle _s) {
    (void)_s;
    return 0.0;
}

int ng_platform_slider_set_enabled(NGHandle _s, int _e) {
    (void)_s;
    (void)_e;
    return NG_SUCCESS;
}

void ng_platform_slider_invalidate(NGHandle _s) {
    (void)_s;
}

NGHandle ng_platform_create_checkbox(const char* _label) {
    (void)_label;
    return NULL;
}

int ng_platform_checkbox_set_checked(NGHandle _c, int _v) {
    (void)_c;
    (void)_v;
    return NG_SUCCESS;
}

int ng_platform_checkbox_get_checked(NGHandle _c) {
    (void)_c;
    return 0;
}

int ng_platform_checkbox_set_enabled(NGHandle _c, int _e) {
    (void)_c;
    (void)_e;
    return NG_SUCCESS;
}

void ng_platform_checkbox_invalidate(NGHandle _c) {
    (void)_c;
}

NGHandle ng_platform_create_progress_bar(void) {
    return NULL;
}

int ng_platform_progress_bar_set_value(NGHandle _p, double _v) {
    (void)_p;
    (void)_v;
    return NG_SUCCESS;
}

int ng_platform_progress_bar_set_indeterminate(NGHandle _p, int _i) {
    (void)_p;
    (void)_i;
    return NG_SUCCESS;
}

int ng_platform_progress_bar_set_enabled(NGHandle _p, int _e) {
    (void)_p;
    (void)_e;
    return NG_SUCCESS;
}

void ng_platform_progress_bar_invalidate(NGHandle _p) {
    (void)_p;
}

NGHandle ng_platform_create_combo_box(void) {
    return NULL;
}

int ng_platform_combo_box_add_item(NGHandle _c, const char* _item) {
    (void)_c;
    (void)_item;
    return NG_SUCCESS;
}

int ng_platform_combo_box_set_selected(NGHandle _c, int _i) {
    (void)_c;
    (void)_i;
    return NG_SUCCESS;
}

int ng_platform_combo_box_get_selected(NGHandle _c) {
    (void)_c;
    return -1;
}

int ng_platform_combo_box_clear(NGHandle _c) {
    (void)_c;
    return NG_SUCCESS;
}

int ng_platform_combo_box_set_enabled(NGHandle _c, int _e) {
    (void)_c;
    (void)_e;
    return NG_SUCCESS;
}

void ng_platform_combo_box_invalidate(NGHandle _c) {
    (void)_c;
}

NGHandle ng_platform_create_tab_bar(unsigned int _id) {
    (void)_id;
    return NULL;
}

int ng_platform_tab_bar_add_tab(NGHandle _t, const char* _title) {
    (void)_t;
    (void)_title;
    return NG_SUCCESS;
}

int ng_platform_tab_bar_remove_tab(NGHandle _t, int _i) {
    (void)_t;
    (void)_i;
    return NG_SUCCESS;
}

int ng_platform_tab_bar_set_selected(NGHandle _t, int _i) {
    (void)_t;
    (void)_i;
    return NG_SUCCESS;
}

int ng_platform_tab_bar_get_selected(NGHandle _t) {
    (void)_t;
    return -1;
}

void ng_platform_tab_bar_invalidate(NGHandle _t) {
    (void)_t;
}

NGHandle ng_platform_create_sidebar_list(unsigned int _id) {
    (void)_id;
    return NULL;
}

int ng_platform_sidebar_list_add_section(NGHandle _s, const char* _title) {
    (void)_s;
    (void)_title;
    return NG_SUCCESS;
}

int ng_platform_sidebar_list_add_item(NGHandle _s, const char* _title, int _indent) {
    (void)_s;
    (void)_title;
    (void)_indent;
    return NG_SUCCESS;
}

int ng_platform_sidebar_list_set_selected(NGHandle _s, int _i) {
    (void)_s;
    (void)_i;
    return NG_SUCCESS;
}

int ng_platform_sidebar_list_get_selected(NGHandle _s) {
    (void)_s;
    return -1;
}

int ng_platform_sidebar_list_clear(NGHandle _s) {
    (void)_s;
    return NG_SUCCESS;
}

void ng_platform_sidebar_list_invalidate(NGHandle _s) {
    (void)_s;
}

NGHandle ng_platform_create_swiftui_host(int _w, int _h) {
    (void)_w;
    (void)_h;
    return NULL;
}
