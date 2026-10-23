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
    return NULL;
}

void ng_ios_destroy_menu(NGMenuHandle handle) {
    (void)handle;
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
    (void)handle;
}

void ng_platform_label_invalidate(NGHandle handle) {
    (void)handle;
}

NGHandle ng_platform_create_scroll_view(void) {
    return NULL;
}

int ng_platform_scroll_view_set_content(NGHandle scroll, NGHandle element) {
    (void)scroll;
    (void)element;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_scroll_view_set_scrollbar(NGHandle scroll, int policy) {
    (void)scroll;
    (void)policy;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_label_set_style(NGHandle label, int style) {
    (void)label;
    (void)style;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_label_set_text(NGHandle handle, const char* text) {
    if (!handle) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    UILabel* label = (__bridge UILabel*)handle;
    [label setText:[NSString stringWithUTF8String:text]];
    return NG_SUCCESS;
}

void ng_platform_canvas_invalidate(NGHandle handle) {
    (void)handle;
}

void ng_platform_text_editor_invalidate(NGHandle handle) {
    (void)handle;
}

void ng_platform_text_view_invalidate(NGHandle handle) {
    (void)handle;
}

void ng_platform_progress_bar_invalidate(NGHandle handle) {
    (void)handle;
}

int ng_platform_element_set_enabled(NGHandle element, int enabled) {
    (void)element;
    (void)enabled;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_element_watch_destroy(NGHandle element, unsigned int id) {
    (void)element;
    (void)id;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_text_unit(void) {
    return NG_TEXT_UNIT_UTF16;
}

int ng_platform_text_get_selection(NGHandle text, int* start, int* end) {
    (void)text;
    (void)start;
    (void)end;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_text_set_selection(NGHandle text, int start, int end) {
    (void)text;
    (void)start;
    (void)end;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_text_replace(NGHandle text, int start, int end, const char* replacement) {
    (void)text;
    (void)start;
    (void)end;
    (void)replacement;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_text_undo(NGHandle text, int redo) {
    (void)text;
    (void)redo;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_text_can_undo(NGHandle text, int redo) {
    (void)text;
    (void)redo;
    return 0;
}

int ng_platform_text_set_feature(NGHandle text, int feature, int on) {
    (void)text;
    (void)feature;
    (void)on;
    return NG_ERROR_UNSUPPORTED;
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

int ng_platform_element_set_accessible_name(NGHandle element, const char* name) {
    (void)element;
    (void)name;
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

