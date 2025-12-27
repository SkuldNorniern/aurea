#include "linux.h"
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

void ng_platform_destroy_window(NGHandle handle) {
    ng_linux_destroy_window(handle);
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

int ng_platform_run(void) {
    gtk_main();
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

float ng_platform_get_scale_factor(NGHandle window) {
    return ng_linux_get_scale_factor(window);
}

typedef void (*ScaleFactorCallback)(void*, float);
void ng_platform_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    ng_linux_window_set_scale_factor_callback(window, callback);
}

