#include "linux.h"
#include <stdint.h>
#include "linux/utils.h"
#include "linux/window.h"
#include "linux/menu.h"
#include "linux/elements.h"
#include "../common/errors.h"
#include <gtk/gtk.h>

int ng_platform_init(void) {
    return ng_linux_init();
}

void ng_platform_cleanup(void) {
    ng_linux_cleanup();
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    return ng_linux_create_window(title, width, height);
}

NGHandle ng_platform_create_window_with_type(const char* title, int width, int height, int window_type) {
    return ng_linux_create_window_with_type(title, width, height, window_type);
}

void ng_platform_destroy_window(NGHandle handle) {
    ng_linux_destroy_window(handle);
}

void ng_platform_window_show(NGHandle window) {
    // TODO: Implement for Linux
    (void)window;
}

void ng_platform_window_hide(NGHandle window) {
    // TODO: Implement for Linux
    (void)window;
}

int ng_platform_window_is_visible(NGHandle window) {
    // TODO: Implement for Linux
    (void)window;
    return 1;
}

NGMenuHandle ng_platform_create_menu(void) {
    return ng_linux_create_menu();
}

void ng_platform_destroy_menu(NGMenuHandle handle) {
    ng_linux_destroy_menu(handle);
}

int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu) {
    return ng_linux_attach_menu(window, menu);
}

int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    return ng_linux_add_menu_item(menu, title, id);
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle parentMenu, const char* title) {
    return ng_linux_create_submenu(parentMenu, title);
}

extern void ng_process_frames(void);

// Idle callback to process frames
static gboolean process_frames_idle(gpointer user_data) {
    (void)user_data;
    ng_process_frames();
    return G_SOURCE_CONTINUE; // Keep the idle source active
}

int ng_platform_run(void) {
    // Add idle callback to process frames
    g_idle_add(process_frames_idle, NULL);
    
    gtk_main();
    return NG_SUCCESS;
}

int ng_platform_poll_events(void) {
    // For now, on Linux with GTK, we rely on the main loop.
    // If we need manual polling, we would use g_main_context_iteration.
    while (g_main_context_iteration(NULL, FALSE));
    return NG_SUCCESS;
}

int ng_platform_set_window_content(NGHandle window, NGHandle content) {
    return ng_linux_set_window_content(window, content);
}

NGHandle ng_platform_create_button(const char* title, unsigned int id) {
    return ng_linux_create_button(title, id);
}

NGHandle ng_platform_create_label(const char* text) {
    return ng_linux_create_label(text);
}

NGHandle ng_platform_create_box(int is_vertical) {
    return ng_linux_create_box(is_vertical);
}

void ng_platform_box_invalidate(NGHandle box) {
    ng_linux_box_invalidate(box);
}

int ng_platform_box_add(NGHandle box, NGHandle element) {
    return ng_linux_box_add(box, element);
}

NGHandle ng_platform_create_text_editor(unsigned int id) {
    return ng_linux_create_text_editor(id);
}

void ng_platform_text_editor_invalidate(NGHandle text_editor) {
    ng_linux_text_editor_invalidate(text_editor);
}

NGHandle ng_platform_create_text_view(int is_editable, unsigned int id) {
    return ng_linux_create_text_view(is_editable, id);
}

void ng_platform_text_view_invalidate(NGHandle text_view) {
    ng_linux_text_view_invalidate(text_view);
}

int ng_platform_set_text_content(NGHandle text_handle, const char* content) {
    return ng_linux_set_text_content(text_handle, content);
}

char* ng_platform_get_text_content(NGHandle text_handle) {
    return ng_linux_get_text_content(text_handle);
}

void ng_platform_free_text_content(char* content) {
    ng_linux_free_text_content(content);
}

NGHandle ng_platform_create_canvas(int width, int height) {
    return ng_linux_create_canvas(width, height);
}

void ng_platform_canvas_invalidate(NGHandle canvas) {
    ng_linux_canvas_invalidate(canvas);
}

void ng_platform_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    ng_linux_canvas_invalidate_rect(canvas, x, y, width, height);
}

void ng_platform_button_invalidate(NGHandle button) {
    ng_linux_button_invalidate(button);
}

void ng_platform_label_invalidate(NGHandle label) {
    ng_linux_label_invalidate(label);
}

NGHandle ng_platform_canvas_get_window(NGHandle canvas) {
    return ng_linux_canvas_get_window(canvas);
}

NGHandle ng_platform_canvas_get_native_handle(NGHandle canvas) {
    return ng_linux_canvas_get_native_handle(canvas);
}

int ng_platform_canvas_get_xcb_handle(NGHandle canvas, uint32_t* xcb_window, void** xcb_connection) {
    return ng_linux_canvas_get_xcb_handle(canvas, xcb_window, xcb_connection);
}

int ng_platform_canvas_get_wayland_handle(NGHandle canvas, void** surface, void** display) {
    return ng_linux_canvas_get_wayland_handle(canvas, surface, display);
}

float ng_platform_get_scale_factor(NGHandle window) {
    return ng_linux_get_scale_factor(window);
}

typedef void (*ScaleFactorCallback)(void*, float);
void ng_platform_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    ng_linux_window_set_scale_factor_callback(window, callback);
}

void ng_platform_window_set_lifecycle_callback(NGHandle window) {
    ng_linux_window_set_lifecycle_callback(window);
}

void ng_platform_window_set_title(NGHandle window, const char* title) {
    ng_linux_window_set_title(window, title);
}

void ng_platform_window_set_size(NGHandle window, int width, int height) {
    ng_linux_window_set_size(window, width, height);
}

void ng_platform_window_get_size(NGHandle window, int* width, int* height) {
    ng_linux_window_get_size(window, width, height);
}

void ng_platform_window_request_close(NGHandle window) {
    ng_linux_window_request_close(window);
}

int ng_platform_window_is_focused(NGHandle window) {
    return ng_linux_window_is_focused(window);
}

int ng_platform_window_set_cursor_visible(NGHandle window, int visible) {
    return ng_linux_window_set_cursor_visible(window, visible);
}

int ng_platform_window_set_cursor_grab(NGHandle window, int mode) {
    return ng_linux_window_set_cursor_grab(window, mode);
}

int ng_platform_window_get_xcb_handle(NGHandle window, uint32_t* xcb_window, void** xcb_connection) {
    return ng_linux_window_get_xcb_handle(window, xcb_window, xcb_connection);
}

int ng_platform_window_get_wayland_handle(NGHandle window, void** surface, void** display) {
    return ng_linux_window_get_wayland_handle(window, surface, display);
}

// ImageView functions
NGHandle ng_platform_create_image_view(void) {
    return ng_linux_create_image_view();
}

int ng_platform_image_view_load_from_path(NGHandle image_view, const char* path) {
    return ng_linux_image_view_load_from_path(image_view, path);
}

int ng_platform_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    return ng_linux_image_view_load_from_data(image_view, data, size);
}

void ng_platform_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    ng_linux_image_view_set_scaling(image_view, scaling_mode);
}

void ng_platform_image_view_invalidate(NGHandle image_view) {
    ng_linux_image_view_invalidate(image_view);
}

// Slider functions
NGHandle ng_platform_create_slider(double min, double max) {
    return ng_linux_create_slider(min, max);
}

int ng_platform_slider_set_value(NGHandle slider, double value) {
    return ng_linux_slider_set_value(slider, value);
}

double ng_platform_slider_get_value(NGHandle slider) {
    return ng_linux_slider_get_value(slider);
}

int ng_platform_slider_set_enabled(NGHandle slider, int enabled) {
    return ng_linux_slider_set_enabled(slider, enabled);
}

void ng_platform_slider_invalidate(NGHandle slider) {
    ng_linux_slider_invalidate(slider);
}

// Checkbox functions
NGHandle ng_platform_create_checkbox(const char* label) {
    return ng_linux_create_checkbox(label);
}

int ng_platform_checkbox_set_checked(NGHandle checkbox, int checked) {
    return ng_linux_checkbox_set_checked(checkbox, checked);
}

int ng_platform_checkbox_get_checked(NGHandle checkbox) {
    return ng_linux_checkbox_get_checked(checkbox);
}

int ng_platform_checkbox_set_enabled(NGHandle checkbox, int enabled) {
    return ng_linux_checkbox_set_enabled(checkbox, enabled);
}

void ng_platform_checkbox_invalidate(NGHandle checkbox) {
    ng_linux_checkbox_invalidate(checkbox);
}

// ProgressBar functions
NGHandle ng_platform_create_progress_bar(void) {
    return ng_linux_create_progress_bar();
}

int ng_platform_progress_bar_set_value(NGHandle progress_bar, double value) {
    return ng_linux_progress_bar_set_value(progress_bar, value);
}

int ng_platform_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    return ng_linux_progress_bar_set_indeterminate(progress_bar, indeterminate);
}

int ng_platform_progress_bar_set_enabled(NGHandle progress_bar, int enabled) {
    return ng_linux_progress_bar_set_enabled(progress_bar, enabled);
}

void ng_platform_progress_bar_invalidate(NGHandle progress_bar) {
    ng_linux_progress_bar_invalidate(progress_bar);
}

// ComboBox functions
NGHandle ng_platform_create_combo_box(void) {
    return ng_linux_create_combo_box();
}

int ng_platform_combo_box_add_item(NGHandle combo_box, const char* item) {
    return ng_linux_combo_box_add_item(combo_box, item);
}

int ng_platform_combo_box_set_selected(NGHandle combo_box, int index) {
    return ng_linux_combo_box_set_selected(combo_box, index);
}

int ng_platform_combo_box_get_selected(NGHandle combo_box) {
    return ng_linux_combo_box_get_selected(combo_box);
}

int ng_platform_combo_box_clear(NGHandle combo_box) {
    return ng_linux_combo_box_clear(combo_box);
}

int ng_platform_combo_box_set_enabled(NGHandle combo_box, int enabled) {
    return ng_linux_combo_box_set_enabled(combo_box, enabled);
}

void ng_platform_combo_box_invalidate(NGHandle combo_box) {
    ng_linux_combo_box_invalidate(combo_box);
}
