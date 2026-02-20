#import "ios/ios.h"
#import "ios/window.h"
#import "ios/elements.h"
#import "../common/errors.h"
#import <UIKit/UIKit.h>

int ng_platform_init(void) {
    return ng_ios_init();
}

void ng_platform_cleanup(void) {
    ng_ios_cleanup();
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    return ng_ios_create_window(title, width, height);
}

void ng_platform_destroy_window(NGHandle handle) {
    ng_ios_destroy_window(handle);
}

NGMenuHandle ng_platform_create_menu(void) {
    return ng_ios_create_menu();
}

void ng_platform_destroy_menu(NGMenuHandle handle) {
    ng_ios_destroy_menu(handle);
}

int ng_platform_attach_menu(NGHandle _window, NGMenuHandle _menu) {
    // iOS doesn't support menu bars
    (void)_window;
    (void)_menu;
    return NG_SUCCESS;
}

int ng_platform_add_menu_item(NGMenuHandle _menu, const char* _title, unsigned int _id) {
    // iOS doesn't support menu bars
    (void)_menu;
    (void)_title;
    (void)_id;
    return NG_SUCCESS;
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle _parentMenu, const char* _title) {
    // iOS doesn't support menu bars
    (void)_parentMenu;
    (void)_title;
    return NULL;
}

int ng_platform_run(void) {
    // iOS app runs via UIApplicationMain in main.m
    // This function should not be called on iOS
    // The event loop is managed by UIKit
    return NG_SUCCESS;
}

int ng_platform_set_window_content(NGHandle window, NGHandle content) {
    return ng_ios_set_window_content(window, content);
}

NGHandle ng_platform_create_button(const char* title, unsigned int id) {
    return ng_ios_create_button(title, id);
}

NGHandle ng_platform_create_label(const char* text) {
    return ng_ios_create_label(text);
}

NGHandle ng_platform_create_box(int is_vertical) {
    return ng_ios_create_box(is_vertical);
}

int ng_platform_box_add(NGHandle box, NGHandle element) {
    return ng_ios_box_add(box, element);
}

NGHandle ng_platform_create_text_editor(unsigned int _id) {
    // TODO: Implement text editor for iOS
    (void)_id;
    return NULL;
}

NGHandle ng_platform_create_text_view(int _is_editable, unsigned int _id) {
    // TODO: Implement text view for iOS
    (void)_is_editable;
    (void)_id;
    return NULL;
}

int ng_platform_set_text_content(NGHandle _text_handle, const char* _content) {
    // TODO: Implement for iOS
    (void)_text_handle;
    (void)_content;
    return NG_SUCCESS;
}

char* ng_platform_get_text_content(NGHandle _text_handle) {
    // TODO: Implement for iOS
    (void)_text_handle;
    return NULL;
}

void ng_platform_free_text_content(char* content) {
    if (content) {
        free(content);
    }
}

NGHandle ng_platform_create_canvas(int width, int height) {
    return ng_ios_create_canvas(width, height);
}

void ng_platform_canvas_invalidate(NGHandle _canvas) {
    // TODO: Implement canvas invalidation for iOS
    (void)_canvas;
}

// ImageView functions
NGHandle ng_platform_create_image_view(void) {
    return ng_ios_create_image_view();
}

int ng_platform_image_view_load_from_path(NGHandle image_view, const char* path) {
    return ng_ios_image_view_load_from_path(image_view, path);
}

int ng_platform_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    return ng_ios_image_view_load_from_data(image_view, data, size);
}

void ng_platform_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    ng_ios_image_view_set_scaling(image_view, scaling_mode);
}

// Slider functions
NGHandle ng_platform_create_slider(double min, double max) {
    return ng_ios_create_slider(min, max);
}

int ng_platform_slider_set_value(NGHandle slider, double value) {
    return ng_ios_slider_set_value(slider, value);
}

double ng_platform_slider_get_value(NGHandle slider) {
    return ng_ios_slider_get_value(slider);
}

int ng_platform_slider_set_enabled(NGHandle slider, int enabled) {
    return ng_ios_slider_set_enabled(slider, enabled);
}

// Checkbox functions
NGHandle ng_platform_create_checkbox(const char* label) {
    return ng_ios_create_checkbox(label);
}

int ng_platform_checkbox_set_checked(NGHandle checkbox, int checked) {
    return ng_ios_checkbox_set_checked(checkbox, checked);
}

int ng_platform_checkbox_get_checked(NGHandle checkbox) {
    return ng_ios_checkbox_get_checked(checkbox);
}

int ng_platform_checkbox_set_enabled(NGHandle checkbox, int enabled) {
    return ng_ios_checkbox_set_enabled(checkbox, enabled);
}

// ProgressBar functions
NGHandle ng_platform_create_progress_bar(void) {
    return ng_ios_create_progress_bar();
}

int ng_platform_progress_bar_set_value(NGHandle progress_bar, double value) {
    return ng_ios_progress_bar_set_value(progress_bar, value);
}

int ng_platform_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    return ng_ios_progress_bar_set_indeterminate(progress_bar, indeterminate);
}

int ng_platform_progress_bar_set_enabled(NGHandle progress_bar, int enabled) {
    return ng_ios_progress_bar_set_enabled(progress_bar, enabled);
}

// ComboBox functions
NGHandle ng_platform_create_combo_box(void) {
    return ng_ios_create_combo_box();
}

int ng_platform_combo_box_add_item(NGHandle combo_box, const char* item) {
    return ng_ios_combo_box_add_item(combo_box, item);
}

int ng_platform_combo_box_set_selected(NGHandle combo_box, int index) {
    return ng_ios_combo_box_set_selected(combo_box, index);
}

int ng_platform_combo_box_get_selected(NGHandle combo_box) {
    return ng_ios_combo_box_get_selected(combo_box);
}

int ng_platform_combo_box_clear(NGHandle combo_box) {
    return ng_ios_combo_box_clear(combo_box);
}

int ng_platform_combo_box_set_enabled(NGHandle combo_box, int enabled) {
    return ng_ios_combo_box_set_enabled(combo_box, enabled);
}

NGHandle ng_platform_create_tab_bar(unsigned int id) {
    (void)id;
    return NULL;
}

int ng_platform_tab_bar_add_tab(NGHandle tab_bar, const char* title) {
    (void)tab_bar;
    (void)title;
    return NG_SUCCESS;
}

int ng_platform_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    (void)tab_bar;
    (void)index;
    return NG_SUCCESS;
}

int ng_platform_tab_bar_set_selected(NGHandle tab_bar, int index) {
    (void)tab_bar;
    (void)index;
    return NG_SUCCESS;
}

int ng_platform_tab_bar_get_selected(NGHandle tab_bar) {
    (void)tab_bar;
    return -1;
}

void ng_platform_tab_bar_invalidate(NGHandle tab_bar) {
    (void)tab_bar;
}

NGHandle ng_platform_create_sidebar_list(unsigned int id) {
    (void)id;
    return NULL;
}

int ng_platform_sidebar_list_add_section(NGHandle sidebar, const char* title) {
    (void)sidebar;
    (void)title;
    return NG_SUCCESS;
}

int ng_platform_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    (void)sidebar;
    (void)title;
    (void)indent;
    return NG_SUCCESS;
}

int ng_platform_sidebar_list_set_selected(NGHandle sidebar, int index) {
    (void)sidebar;
    (void)index;
    return NG_SUCCESS;
}

int ng_platform_sidebar_list_get_selected(NGHandle sidebar) {
    (void)sidebar;
    return -1;
}

int ng_platform_sidebar_list_clear(NGHandle sidebar) {
    (void)sidebar;
    return NG_SUCCESS;
}

void ng_platform_sidebar_list_invalidate(NGHandle sidebar) {
    (void)sidebar;
}

float ng_platform_get_scale_factor(NGHandle window) {
    return ng_ios_get_scale_factor(window);
}

void ng_platform_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    ng_ios_window_set_scale_factor_callback(window, callback);
}

void ng_platform_window_set_lifecycle_callback(NGHandle window) {
    ng_ios_window_set_lifecycle_callback(window);
}

void ng_platform_window_set_title(NGHandle window, const char* title) {
    ng_ios_window_set_title(window, title);
}

void ng_platform_window_set_size(NGHandle window, int width, int height) {
    ng_ios_window_set_size(window, width, height);
}

void ng_platform_window_get_size(NGHandle window, int* width, int* height) {
    ng_ios_window_get_size(window, width, height);
}

void ng_platform_window_request_close(NGHandle window) {
    ng_ios_window_request_close(window);
}

int ng_platform_window_is_focused(NGHandle window) {
    return ng_ios_window_is_focused(window);
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
