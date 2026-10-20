#import "ios.h"
#import "window.h"
#import "elements.h"
#import "../../common/errors.h"
#import <UIKit/UIKit.h>

int ng_ios_init(void) {
    return NG_SUCCESS;
}

void ng_ios_cleanup(void) {
}

NGHandle ng_ios_create_window(const char* title, int width, int height) {
    return ng_ios_create_window_impl(title, width, height);
}

void ng_ios_destroy_window(NGHandle handle) {
    ng_ios_destroy_window_impl(handle);
}

NGMenuHandle ng_ios_create_menu(void) {
    // TODO: Create iOS menu (limited support)
    return NULL;
}

void ng_ios_destroy_menu(NGMenuHandle handle) {
    // TODO: Destroy iOS menu
}

NGHandle ng_ios_create_button(const char* title, unsigned int id) {
    return ng_ios_create_button_impl(title, id);
}

NGHandle ng_ios_create_label(const char* text) {
    return ng_ios_create_label_impl(text);
}

NGHandle ng_ios_create_canvas(int width, int height) {
    return ng_ios_create_canvas_impl(width, height);
}

void ng_platform_button_invalidate(NGHandle handle) {
}

void ng_platform_label_invalidate(NGHandle handle) {
}

int ng_platform_label_set_text(NGHandle handle, const char* text) {
    if (!handle) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    UILabel* label = (__bridge UILabel*)handle;
    [label setText:[NSString stringWithUTF8String:text]];
    return NG_SUCCESS;
}

void ng_platform_canvas_invalidate(NGHandle handle) {
}

void ng_platform_text_editor_invalidate(NGHandle handle) {
}

void ng_platform_text_view_invalidate(NGHandle handle) {
}

void ng_platform_progress_bar_invalidate(NGHandle handle) {
}

int ng_platform_element_set_change_id(NGHandle element, unsigned int id) {
    (void)element;
    (void)id;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_element_set_visible(NGHandle element, int visible) {
    (void)element;
    (void)visible;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_element_set_min_size(NGHandle element, int width, int height) {
    (void)element;
    (void)width;
    (void)height;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_box_set_spacing(NGHandle box, int spacing) {
    (void)box;
    (void)spacing;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_box_set_padding(NGHandle box, int padding) {
    (void)box;
    (void)padding;
    return NG_ERROR_UNSUPPORTED;
}

