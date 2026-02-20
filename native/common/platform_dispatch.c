#include "platform_api.h"
#include "platform_ops.h"
#include "errors.h"
#include <stdint.h>

static const ng_platform_ops_t* g_ops = NULL;

void ng_platform_register_ops(const ng_platform_ops_t* ops) {
    g_ops = ops;
}

static void ensure_ops(void) {
    if (g_ops != NULL) return;
#if defined(__APPLE__)
#ifdef TARGET_OS_IPHONE
    extern void ios_register_ops(void);
    ios_register_ops();
#else
    extern void macos_register_ops(void);
    macos_register_ops();
#endif
#elif defined(_WIN32)
    extern void windows_register_ops(void);
    windows_register_ops();
#elif defined(__linux__)
    extern void linux_register_ops(void);
    linux_register_ops();
#endif
}

#define DISPATCH_INIT(ret, name, ...) do { ensure_ops(); if (!g_ops->name) return (ret)(0); return g_ops->name(__VA_ARGS__); } while(0)
#define DISPATCH_VOID(name, ...) do { ensure_ops(); if (g_ops->name) g_ops->name(__VA_ARGS__); } while(0)
#define DISPATCH_INT(name, ...) do { ensure_ops(); return g_ops->name ? g_ops->name(__VA_ARGS__) : NG_ERROR_PLATFORM_SPECIFIC; } while(0)

int ng_platform_init(void) {
    ensure_ops();
    return g_ops && g_ops->init ? g_ops->init() : NG_SUCCESS;
}

void ng_platform_cleanup(void) {
    if (g_ops && g_ops->cleanup) g_ops->cleanup();
}

int ng_platform_run(void) {
    ensure_ops();
    return g_ops && g_ops->run ? g_ops->run() : NG_SUCCESS;
}

int ng_platform_poll_events(void) {
    ensure_ops();
    return g_ops && g_ops->poll_events ? g_ops->poll_events() : NG_SUCCESS;
}

NGHandle ng_platform_create_window(const char* t, int w, int h) {
    DISPATCH_INIT(NGHandle, create_window, t, w, h);
}

NGHandle ng_platform_create_window_with_type(const char* t, int w, int h, int wt) {
    DISPATCH_INIT(NGHandle, create_window_with_type, t, w, h, wt);
}

void ng_platform_destroy_window(NGHandle h) {
    DISPATCH_VOID(destroy_window, h);
}

void ng_platform_window_set_title(NGHandle w, const char* t) {
    DISPATCH_VOID(window_set_title, w, t);
}

void ng_platform_window_set_size(NGHandle w, int wd, int h) {
    DISPATCH_VOID(window_set_size, w, wd, h);
}

void ng_platform_window_get_size(NGHandle w, int* wd, int* h) {
    DISPATCH_VOID(window_get_size, w, wd, h);
}

void ng_platform_window_request_close(NGHandle w) {
    DISPATCH_VOID(window_request_close, w);
}

int ng_platform_window_is_focused(NGHandle w) {
    DISPATCH_INIT(int, window_is_focused, w);
}

int ng_platform_window_set_cursor_visible(NGHandle w, int v) {
    DISPATCH_INT(window_set_cursor_visible, w, v);
}

int ng_platform_window_set_cursor_grab(NGHandle w, int m) {
    DISPATCH_INT(window_set_cursor_grab, w, m);
}

NGHandle ng_platform_window_get_content_view(NGHandle w) {
    DISPATCH_INIT(NGHandle, window_get_content_view, w);
}

void ng_platform_window_show(NGHandle w) {
    DISPATCH_VOID(window_show, w);
}

void ng_platform_window_hide(NGHandle w) {
    DISPATCH_VOID(window_hide, w);
}

int ng_platform_window_is_visible(NGHandle w) {
    DISPATCH_INIT(int, window_is_visible, w);
}

void ng_platform_window_set_position(NGHandle w, int x, int y) {
    DISPATCH_VOID(window_set_position, w, x, y);
}

void ng_platform_window_get_position(NGHandle w, int* x, int* y) {
    DISPATCH_VOID(window_get_position, w, x, y);
}

#if defined(__linux__)
int ng_platform_window_get_xcb_handle(NGHandle w, uint32_t* xw, void** xc) {
    DISPATCH_INT(window_get_xcb_handle, w, xw, xc);
}

int ng_platform_window_get_wayland_handle(NGHandle w, void** s, void** d) {
    DISPATCH_INT(window_get_wayland_handle, w, s, d);
}
#endif

NGMenuHandle ng_platform_create_menu(void) {
    DISPATCH_INIT(NGMenuHandle, create_menu);
}

void ng_platform_destroy_menu(NGMenuHandle h) {
    DISPATCH_VOID(destroy_menu, h);
}

int ng_platform_attach_menu(NGHandle w, NGMenuHandle m) {
    DISPATCH_INT(attach_menu, w, m);
}

int ng_platform_add_menu_item(NGMenuHandle m, const char* t, unsigned int id) {
    DISPATCH_INT(add_menu_item, m, t, id);
}

int ng_platform_add_menu_separator(NGMenuHandle m) {
    DISPATCH_INT(add_menu_separator, m);
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle p, const char* t) {
    DISPATCH_INIT(NGMenuHandle, create_submenu, p, t);
}

NGHandle ng_platform_create_button(const char* t, unsigned int id) {
    DISPATCH_INIT(NGHandle, create_button, t, id);
}

void ng_platform_button_invalidate(NGHandle b) {
    DISPATCH_VOID(button_invalidate, b);
}

NGHandle ng_platform_create_label(const char* t) {
    DISPATCH_INIT(NGHandle, create_label, t);
}

void ng_platform_label_invalidate(NGHandle l) {
    DISPATCH_VOID(label_invalidate, l);
}

NGHandle ng_platform_create_box(int v) {
    DISPATCH_INIT(NGHandle, create_box, v);
}

void ng_platform_box_invalidate(NGHandle b) {
    DISPATCH_VOID(box_invalidate, b);
}

int ng_platform_box_add(NGHandle b, NGHandle e, float w) {
    DISPATCH_INT(box_add, b, e, w);
}

int ng_platform_set_window_content(NGHandle w, NGHandle c) {
    DISPATCH_INT(set_window_content, w, c);
}

NGHandle ng_platform_create_split_view(int v) {
    DISPATCH_INIT(NGHandle, create_split_view, v);
}

int ng_platform_split_view_add(NGHandle s, NGHandle e) {
    DISPATCH_INT(split_view_add, s, e);
}

int ng_platform_split_view_set_divider_position(NGHandle s, int i, float p) {
    DISPATCH_INT(split_view_set_divider_position, s, i, p);
}

NGHandle ng_platform_create_text_editor(unsigned int id) {
    DISPATCH_INIT(NGHandle, create_text_editor, id);
}

void ng_platform_text_editor_invalidate(NGHandle h) {
    DISPATCH_VOID(text_editor_invalidate, h);
}

NGHandle ng_platform_create_text_view(int e, unsigned int id) {
    DISPATCH_INIT(NGHandle, create_text_view, e, id);
}

void ng_platform_text_view_invalidate(NGHandle h) {
    DISPATCH_VOID(text_view_invalidate, h);
}

NGHandle ng_platform_create_text_field(void) {
    DISPATCH_INIT(NGHandle, create_text_field);
}

int ng_platform_set_text_content(NGHandle h, const char* c) {
    DISPATCH_INT(set_text_content, h, c);
}

char* ng_platform_get_text_content(NGHandle h) {
    DISPATCH_INIT(char*, get_text_content, h);
}

void ng_platform_free_text_content(char* c) {
    DISPATCH_VOID(free_text_content, c);
}

NGHandle ng_platform_create_canvas(int w, int h) {
    DISPATCH_INIT(NGHandle, create_canvas, w, h);
}

void ng_platform_canvas_invalidate(NGHandle c) {
    DISPATCH_VOID(canvas_invalidate, c);
}

void ng_platform_canvas_invalidate_rect(NGHandle c, float x, float y, float w, float h) {
    DISPATCH_VOID(canvas_invalidate_rect, c, x, y, w, h);
}

void ng_platform_canvas_update_buffer(NGHandle c, const unsigned char* b, unsigned int sz, unsigned int w, unsigned int h) {
    DISPATCH_VOID(canvas_update_buffer, c, b, sz, w, h);
}

void ng_platform_canvas_get_size(NGHandle c, unsigned int* w, unsigned int* h) {
    DISPATCH_VOID(canvas_get_size, c, w, h);
}

NGHandle ng_platform_canvas_get_window(NGHandle c) {
    DISPATCH_INIT(NGHandle, canvas_get_window, c);
}

NGHandle ng_platform_canvas_get_native_handle(NGHandle c) {
    DISPATCH_INIT(NGHandle, canvas_get_native_handle, c);
}

#if defined(__linux__)
int ng_platform_canvas_get_xcb_handle(NGHandle c, uint32_t* xw, void** xc) {
    DISPATCH_INT(canvas_get_xcb_handle, c, xw, xc);
}

int ng_platform_canvas_get_wayland_handle(NGHandle c, void** s, void** d) {
    DISPATCH_INT(canvas_get_wayland_handle, c, s, d);
}
#endif

float ng_platform_get_scale_factor(NGHandle w) {
    ensure_ops();
    return g_ops && g_ops->get_scale_factor ? g_ops->get_scale_factor(w) : 1.0f;
}

void ng_platform_window_set_scale_factor_callback(NGHandle w, ScaleFactorCallback cb) {
    DISPATCH_VOID(window_set_scale_factor_callback, w, cb);
}

void ng_platform_window_set_lifecycle_callback(NGHandle w) {
    DISPATCH_VOID(window_set_lifecycle_callback, w);
}

NGHandle ng_platform_create_image_view(void) {
    DISPATCH_INIT(NGHandle, create_image_view);
}

int ng_platform_image_view_load_from_path(NGHandle v, const char* p) {
    DISPATCH_INT(image_view_load_from_path, v, p);
}

int ng_platform_image_view_load_from_data(NGHandle v, const unsigned char* d, unsigned int s) {
    DISPATCH_INT(image_view_load_from_data, v, d, s);
}

void ng_platform_image_view_set_scaling(NGHandle v, int m) {
    DISPATCH_VOID(image_view_set_scaling, v, m);
}

void ng_platform_image_view_invalidate(NGHandle v) {
    DISPATCH_VOID(image_view_invalidate, v);
}

NGHandle ng_platform_create_slider(double mn, double mx) {
    DISPATCH_INIT(NGHandle, create_slider, mn, mx);
}

int ng_platform_slider_set_value(NGHandle s, double v) {
    DISPATCH_INT(slider_set_value, s, v);
}

double ng_platform_slider_get_value(NGHandle s) {
    ensure_ops();
    return g_ops && g_ops->slider_get_value ? g_ops->slider_get_value(s) : 0.0;
}

int ng_platform_slider_set_enabled(NGHandle s, int e) {
    DISPATCH_INT(slider_set_enabled, s, e);
}

void ng_platform_slider_invalidate(NGHandle s) {
    DISPATCH_VOID(slider_invalidate, s);
}

NGHandle ng_platform_create_checkbox(const char* l) {
    DISPATCH_INIT(NGHandle, create_checkbox, l);
}

int ng_platform_checkbox_set_checked(NGHandle c, int v) {
    DISPATCH_INT(checkbox_set_checked, c, v);
}

int ng_platform_checkbox_get_checked(NGHandle c) {
    DISPATCH_INIT(0, checkbox_get_checked, c);
}

int ng_platform_checkbox_set_enabled(NGHandle c, int e) {
    DISPATCH_INT(checkbox_set_enabled, c, e);
}

void ng_platform_checkbox_invalidate(NGHandle c) {
    DISPATCH_VOID(checkbox_invalidate, c);
}

NGHandle ng_platform_create_progress_bar(void) {
    DISPATCH_INIT(NGHandle, create_progress_bar);
}

int ng_platform_progress_bar_set_value(NGHandle p, double v) {
    DISPATCH_INT(progress_bar_set_value, p, v);
}

int ng_platform_progress_bar_set_indeterminate(NGHandle p, int i) {
    DISPATCH_INT(progress_bar_set_indeterminate, p, i);
}

int ng_platform_progress_bar_set_enabled(NGHandle p, int e) {
    DISPATCH_INT(progress_bar_set_enabled, p, e);
}

void ng_platform_progress_bar_invalidate(NGHandle p) {
    DISPATCH_VOID(progress_bar_invalidate, p);
}

NGHandle ng_platform_create_combo_box(void) {
    DISPATCH_INIT(NGHandle, create_combo_box);
}

int ng_platform_combo_box_add_item(NGHandle c, const char* i) {
    DISPATCH_INT(combo_box_add_item, c, i);
}

int ng_platform_combo_box_set_selected(NGHandle c, int i) {
    DISPATCH_INT(combo_box_set_selected, c, i);
}

int ng_platform_combo_box_get_selected(NGHandle c) {
    DISPATCH_INIT(-1, combo_box_get_selected, c);
}

int ng_platform_combo_box_clear(NGHandle c) {
    DISPATCH_INT(combo_box_clear, c);
}

int ng_platform_combo_box_set_enabled(NGHandle c, int e) {
    DISPATCH_INT(combo_box_set_enabled, c, e);
}

void ng_platform_combo_box_invalidate(NGHandle c) {
    DISPATCH_VOID(combo_box_invalidate, c);
}

NGHandle ng_platform_create_tab_bar(unsigned int id) {
    DISPATCH_INIT((NGHandle)NULL, create_tab_bar, id);
}

int ng_platform_tab_bar_add_tab(NGHandle t, const char* ti) {
    DISPATCH_INT(tab_bar_add_tab, t, ti);
}

int ng_platform_tab_bar_remove_tab(NGHandle t, int i) {
    DISPATCH_INT(tab_bar_remove_tab, t, i);
}

int ng_platform_tab_bar_set_selected(NGHandle t, int i) {
    DISPATCH_INT(tab_bar_set_selected, t, i);
}

int ng_platform_tab_bar_get_selected(NGHandle t) {
    DISPATCH_INIT(-1, tab_bar_get_selected, t);
}

void ng_platform_tab_bar_invalidate(NGHandle t) {
    DISPATCH_VOID(tab_bar_invalidate, t);
}

NGHandle ng_platform_create_sidebar_list(unsigned int id) {
    DISPATCH_INIT((NGHandle)NULL, create_sidebar_list, id);
}

int ng_platform_sidebar_list_add_section(NGHandle s, const char* t) {
    DISPATCH_INT(sidebar_list_add_section, s, t);
}

int ng_platform_sidebar_list_add_item(NGHandle s, const char* t, int i) {
    DISPATCH_INT(sidebar_list_add_item, s, t, i);
}

int ng_platform_sidebar_list_set_selected(NGHandle s, int i) {
    DISPATCH_INT(sidebar_list_set_selected, s, i);
}

int ng_platform_sidebar_list_get_selected(NGHandle s) {
    DISPATCH_INIT(-1, sidebar_list_get_selected, s);
}

int ng_platform_sidebar_list_clear(NGHandle s) {
    DISPATCH_INT(sidebar_list_clear, s);
}

void ng_platform_sidebar_list_invalidate(NGHandle s) {
    DISPATCH_VOID(sidebar_list_invalidate, s);
}
