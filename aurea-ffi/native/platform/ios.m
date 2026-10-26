#import "ios/ios.h"
#include <stdlib.h>
#import "ios/window.h"
#import "ios/elements.h"
#import "../common/errors.h"
#import "../common/clipboard.h"
#import <UIKit/UIKit.h>
#include <math.h>
#include <string.h>

int ng_platform_wheel_lines(void) {
    return 3;
}

int ng_platform_prefers_dark(void) {
    return -1;
}

int ng_platform_reduced_motion(void) {
    return UIAccessibilityIsReduceMotionEnabled() ? 1 : 0;
}

int ng_platform_locales(char* buf, int cap) {
    int need = 0;
    for (NSString* tag in [NSLocale preferredLanguages]) {
        need += (int)strlen(tag.UTF8String) + 1;
    }
    if (buf && need > 0 && cap >= need) {
        char* at = buf;
        for (NSString* tag in [NSLocale preferredLanguages]) {
            size_t n = strlen(tag.UTF8String) + 1;
            memcpy(at, tag.UTF8String, n);
            at += n;
        }
    }
    return need;
}

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
    (void)_window;
    (void)_menu;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_add_menu_item(NGMenuHandle _menu, const char* _title, unsigned int _id) {
    (void)_menu;
    (void)_title;
    (void)_id;
    return NG_ERROR_UNSUPPORTED;
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle _parentMenu, const char* _title) {
    (void)_parentMenu;
    (void)_title;
    return NULL;
}

int ng_platform_run(void) {
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_poll_events(void) {
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_wait_events(int timeout_ms) {
    (void)timeout_ms;
    return NG_ERROR_UNSUPPORTED;
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

int ng_platform_box_add(NGHandle box, NGHandle element, float weight) {
    (void)weight;
    return ng_ios_box_add(box, element);
}

NGHandle ng_platform_create_text_editor(void) {
    return NULL;
}

NGHandle ng_platform_create_text_view(int _is_editable) {
    (void)_is_editable;
    return NULL;
}

int ng_platform_set_text_content(NGHandle _text_handle, const char* _content) {
    (void)_text_handle;
    (void)_content;
    return NG_ERROR_UNSUPPORTED;
}

char* ng_platform_get_text_content(NGHandle _text_handle) {
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

void ng_platform_canvas_invalidate(NGHandle canvas) {
    if (canvas) [(__bridge UIView*)canvas setNeedsDisplay];
}

void ng_platform_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    if (canvas) [(__bridge UIView*)canvas setNeedsDisplayInRect:CGRectMake(x, y, width, height)];
}

void ng_platform_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height) {
    (void)canvas; (void)buffer; (void)size; (void)width; (void)height;
}

void* ng_platform_canvas_acquire_buffer(NGHandle canvas, unsigned int width, unsigned int height, unsigned int* stride_px, unsigned int* buffer_index) {
    (void)canvas; (void)width; (void)height; (void)stride_px; (void)buffer_index;
    return NULL;
}

void ng_platform_canvas_present(NGHandle canvas) {
    (void)canvas;
}

void ng_platform_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    CGSize size = canvas ? ((__bridge UIView*)canvas).bounds.size : CGSizeZero;
    if (width) *width = (unsigned int)size.width;
    if (height) *height = (unsigned int)size.height;
}

void ng_platform_canvas_get_pixel_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    UIView* view = (__bridge UIView*)canvas;
    CGFloat scale = view ? view.contentScaleFactor : 1.0;
    CGSize size = view ? view.bounds.size : CGSizeZero;
    if (width) *width = (unsigned int)lround(size.width * scale);
    if (height) *height = (unsigned int)lround(size.height * scale);
}

NGHandle ng_platform_canvas_get_window(NGHandle canvas) {
    return canvas ? (__bridge NGHandle)((__bridge UIView*)canvas).window : NULL;
}

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

int ng_platform_image_view_get_size(NGHandle image_view, unsigned int* width, unsigned int* height) {
    (void)image_view;
    (void)width;
    (void)height;
    return NG_ERROR_UNSUPPORTED;
}

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
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    (void)tab_bar;
    (void)index;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_tab_bar_set_selected(NGHandle tab_bar, int index) {
    (void)tab_bar;
    (void)index;
    return NG_ERROR_UNSUPPORTED;
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
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    (void)sidebar;
    (void)title;
    (void)indent;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_sidebar_list_set_selected(NGHandle sidebar, int index) {
    (void)sidebar;
    (void)index;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_sidebar_list_get_selected(NGHandle sidebar) {
    (void)sidebar;
    return -1;
}

int ng_platform_sidebar_list_clear(NGHandle sidebar) {
    (void)sidebar;
    return NG_ERROR_UNSUPPORTED;
}

void ng_platform_sidebar_list_invalidate(NGHandle sidebar) {
    (void)sidebar;
}

NGHandle ng_platform_create_table(unsigned int id) {
    (void)id;
    return NULL;
}

int ng_platform_table_add_column(NGHandle table, const char* title) {
    (void)table;
    (void)title;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_table_insert_row(NGHandle table, int index, const char* const* cells, int count) {
    (void)table;
    (void)index;
    (void)cells;
    (void)count;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_table_set_row(NGHandle table, int index, const char* const* cells, int count) {
    (void)table;
    (void)index;
    (void)cells;
    (void)count;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_table_remove_row(NGHandle table, int index) {
    (void)table;
    (void)index;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_table_clear(NGHandle table) {
    (void)table;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_table_set_selected(NGHandle table, int index) {
    (void)table;
    (void)index;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_table_get_selected(NGHandle table) {
    (void)table;
    return -1;
}

NGHandle ng_platform_create_row_list(unsigned int id) {
    (void)id;
    return NULL;
}

int ng_platform_row_list_insert(NGHandle list, int index, NGHandle row) {
    (void)list;
    (void)index;
    (void)row;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_row_list_remove(NGHandle list, int index) {
    (void)list;
    (void)index;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_row_list_clear(NGHandle list) {
    (void)list;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_row_list_set_selected(NGHandle list, int index) {
    (void)list;
    (void)index;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_row_list_get_selected(NGHandle list) {
    (void)list;
    return -1;
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

int ng_platform_window_set_min_size(NGHandle window, int width, int height) {
    (void)window;
    (void)width;
    (void)height;
    return NG_ERROR_UNSUPPORTED;
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

void ng_platform_window_set_close_guard(NGHandle window, int guarded) {
    (void)window;
    (void)guarded;
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

int ng_platform_window_set_cursor_icon(NGHandle window, int icon) {
    (void)window;
    (void)icon;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_platform_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height) {
    (void)window;
    (void)x;
    (void)y;
    (void)width;
    (void)height;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_platform_window_set_shortcuts(NGHandle window, const unsigned int* keycodes, const unsigned int* modifiers, int count) {
    (void)window;
    (void)keycodes;
    (void)modifiers;
    (void)count;
    return NG_ERROR_PLATFORM_SPECIFIC;
}

NGHandle ng_platform_create_swiftui_host(int width, int height) {
    (void)width;
    (void)height;
    return NULL;
}

int ng_platform_add_menu_check_item(NGMenuHandle m, const char* t, unsigned int id, int c, int r) {
    (void)m; (void)t; (void)id; (void)c; (void)r;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_menu_item_set_enabled(NGMenuHandle m, unsigned int id, int e) {
    (void)m; (void)id; (void)e;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_menu_item_set_checked(NGMenuHandle m, unsigned int id, int c) {
    (void)m; (void)id; (void)c;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_menu_item_set_title(NGMenuHandle m, unsigned int id, const char* t) {
    (void)m; (void)id; (void)t;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_menu_item_remove(NGMenuHandle m, unsigned int id) {
    (void)m; (void)id;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_menu_clear(NGMenuHandle m) {
    (void)m;
    return NG_ERROR_UNSUPPORTED;
}

char* ng_platform_file_dialog(NGHandle window, int kind, const char* title, const char* filters, const char* default_name, const char* directory, int multiple) {
    (void)window; (void)kind; (void)title; (void)filters;
    (void)default_name; (void)directory; (void)multiple;
    return NULL;
}

void ng_platform_free_dialog_paths(char* paths) {
    free(paths);
}

int ng_platform_message_box(NGHandle window, int kind, const char* title, const char* text, int buttons) {
    (void)window; (void)kind; (void)title; (void)text; (void)buttons;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_color_dialog(NGHandle window, const char* title, unsigned int argb_in, int alpha, unsigned int* argb_out) {
    (void)window; (void)title; (void)argb_in; (void)alpha; (void)argb_out;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_window_set_cursor_rgba(NGHandle window, const unsigned char* rgba, unsigned int width, unsigned int height, unsigned int hot_x, unsigned int hot_y) {
    (void)window; (void)rgba; (void)width; (void)height; (void)hot_x; (void)hot_y;
    return NG_ERROR_UNSUPPORTED;
}

void ng_platform_destroy_element(NGHandle element) {
    if (!element) return;
    UIView* view = (__bridge_transfer UIView*)element;
    if ([view isKindOfClass:[UIView class]]) [view removeFromSuperview];
}

void ng_platform_frame_idle(void) {
}

void ng_platform_request_frame(void) {
}

NGHandle ng_platform_create_window_with_type(const char* title, int width, int height, int window_type) {
    (void)window_type;
    return ng_platform_create_window(title, width, height);
}

NGHandle ng_platform_window_get_content_view(NGHandle window) {
    if (!window) return NULL;
    return (__bridge NGHandle)((__bridge UIWindow*)window).rootViewController.view;
}

void ng_platform_window_show(NGHandle window) {
    if (window) ((__bridge UIWindow*)window).hidden = NO;
}

void ng_platform_window_hide(NGHandle window) {
    if (window) ((__bridge UIWindow*)window).hidden = YES;
}

int ng_platform_window_is_visible(NGHandle window) {
    return window && !((__bridge UIWindow*)window).hidden ? 1 : 0;
}

int ng_platform_window_occlusion(NGHandle window) {
    return ng_platform_window_is_visible(window) ? 0 : 2;
}

int ng_platform_window_set_hide_on_close(NGHandle window, int hide) {
    (void)window; (void)hide;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_window_set_icon_rgba(NGHandle window, const unsigned char* rgba, unsigned int width, unsigned int height) {
    (void)window; (void)rgba; (void)width; (void)height;
    return NG_ERROR_UNSUPPORTED;
}

void ng_platform_window_set_position(NGHandle window, int x, int y) {
    (void)window; (void)x; (void)y;
}

void ng_platform_window_get_position(NGHandle window, int* x, int* y) {
    (void)window;
    if (x) *x = 0;
    if (y) *y = 0;
}

int ng_platform_window_get_xcb_handle(NGHandle window, uint32_t* xcb_window, void** xcb_connection) {
    (void)window; (void)xcb_window; (void)xcb_connection;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_window_get_wayland_handle(NGHandle window, void** surface, void** display) {
    (void)window; (void)surface; (void)display;
    return NG_ERROR_UNSUPPORTED;
}

NGHandle ng_platform_create_tray(const char* tooltip, unsigned int click_id) {
    (void)tooltip; (void)click_id;
    return NULL;
}

void ng_platform_destroy_tray(NGHandle tray) {
    (void)tray;
}

int ng_platform_tray_set_icon_rgba(NGHandle tray, const unsigned char* rgba, unsigned int width, unsigned int height) {
    (void)tray; (void)rgba; (void)width; (void)height;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_tray_set_tooltip(NGHandle tray, const char* tooltip) {
    (void)tray; (void)tooltip;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_tray_add_item(NGHandle tray, const char* title, unsigned int id) {
    (void)tray; (void)title; (void)id;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_tray_notify(NGHandle tray, const char* title, const char* body) {
    (void)tray; (void)title; (void)body;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_add_menu_separator(NGMenuHandle menu) {
    (void)menu;
    return NG_ERROR_UNSUPPORTED;
}

void ng_platform_box_invalidate(NGHandle box) {
    if (box) [(__bridge UIView*)box setNeedsLayout];
}

void ng_platform_image_view_invalidate(NGHandle image_view) {
    if (image_view) [(__bridge UIView*)image_view setNeedsDisplay];
}

void ng_platform_slider_invalidate(NGHandle slider) {
    if (slider) [(__bridge UIView*)slider setNeedsDisplay];
}

void ng_platform_checkbox_invalidate(NGHandle checkbox) {
    if (checkbox) [(__bridge UIView*)checkbox setNeedsDisplay];
}

void ng_platform_combo_box_invalidate(NGHandle combo_box) {
    if (combo_box) [(__bridge UIView*)combo_box setNeedsDisplay];
}

int ng_platform_detach_element(NGHandle element) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    UIView* view = (__bridge UIView*)element;
    if (view.superview) {
        CFRetain(element);
        [view removeFromSuperview];
    }
    return NG_SUCCESS;
}

void ng_platform_element_released_to_parent(NGHandle element) {
    if (element) CFRelease(element);
}

void ng_platform_element_retain(NGHandle element) {
    if (element) CFRetain(element);
}

void ng_platform_element_release(NGHandle element) {
    if (element) CFRelease(element);
}

NGHandle ng_platform_creation_parent(void) {
    return NULL;
}

NGHandle ng_platform_create_split_view(int is_vertical) {
    (void)is_vertical;
    return NULL;
}

int ng_platform_split_view_add(NGHandle split_handle, NGHandle element) {
    (void)split_handle; (void)element;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_split_view_set_divider_position(NGHandle split_handle, int index, float position) {
    (void)split_handle; (void)index; (void)position;
    return NG_ERROR_UNSUPPORTED;
}

NGHandle ng_platform_create_text_field(void) {
    return NULL;
}

void ng_platform_canvas_set_gpu_owned(NGHandle canvas, int gpu_owned) {
    (void)canvas; (void)gpu_owned;
}

NGHandle ng_platform_canvas_get_native_handle(NGHandle canvas) {
    return canvas;
}

int ng_platform_canvas_get_xcb_handle(NGHandle canvas, uint32_t* xcb_window, void** xcb_connection) {
    (void)canvas; (void)xcb_window; (void)xcb_connection;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_canvas_get_wayland_handle(NGHandle canvas, void** surface, void** display) {
    (void)canvas; (void)surface; (void)display;
    return NG_ERROR_UNSUPPORTED;
}

char* ng_platform_get_clipboard_text(void) {
    NSString* text = [UIPasteboard generalPasteboard].string;
    return text ? strdup(text.UTF8String) : NULL;
}

void ng_platform_free_clipboard_text(char* text) {
    free(text);
}

int ng_platform_set_clipboard_text(const char* text) {
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    [UIPasteboard generalPasteboard].string = [NSString stringWithUTF8String:text] ?: @"";
    return NG_SUCCESS;
}

/* Clipboard formats are MIME-style names for images and the app's own type names otherwise; iOS
   calls a PNG `public.png`. */
static NSString* ng_ios_pasteboard_type(const char* format) {
    if (!strcmp(format, "image/png")) return @"public.png";
    return [NSString stringWithUTF8String:format];
}

int ng_platform_clipboard_formats(size_t max_bytes, unsigned char** data, size_t* len) {
    @autoreleasepool {
        int result = NG_SUCCESS;
        for (NSString* type in [UIPasteboard generalPasteboard].pasteboardTypes) {
            const char* name = [type isEqualToString:@"public.png"] ? "image/png" : type.UTF8String;
            if (name && (result = ng_clipboard_append(data, len, name, max_bytes))) break;
        }
        return result;
    }
}

int ng_platform_clipboard_read(const char* format, size_t max_bytes, unsigned char** data, size_t* len) {
    @autoreleasepool {
        if (!format) return NG_ERROR_INVALID_PARAMETER;
        NSString* type = ng_ios_pasteboard_type(format);
        if (!type) return NG_ERROR_INVALID_PARAMETER;
        UIPasteboard* clipboard = [UIPasteboard generalPasteboard];
        NSData* bytes = [clipboard dataForPasteboardType:type];
        /* A picture copied as JPEG or HEIC still pastes as PNG. */
        if (!bytes && !strcmp(format, "image/png") && clipboard.hasImages) {
            UIImage* image = clipboard.image;
            if (image) {
                CGImageRef pixels = image.CGImage;
                if (pixels && CGImageGetHeight(pixels) &&
                    CGImageGetWidth(pixels) > max_bytes / 4 / CGImageGetHeight(pixels)) {
                    return NG_ERROR_LIMIT_EXCEEDED;
                }
                bytes = UIImagePNGRepresentation(image);
            }
        }
        if (!bytes) return NG_SUCCESS;
        if (bytes.length > max_bytes) return NG_ERROR_LIMIT_EXCEEDED;
        unsigned char* copy = (unsigned char*)malloc(bytes.length ? bytes.length : 1);
        if (!copy) return NG_ERROR_PLATFORM_SPECIFIC;
        if (bytes.length) memcpy(copy, bytes.bytes, bytes.length);
        *data = copy;
        *len = bytes.length;
        return NG_SUCCESS;
    }
}

int ng_platform_clipboard_write(const NGClipboardEntry* entries, size_t count) {
    @autoreleasepool {
        UIPasteboard* clipboard = [UIPasteboard generalPasteboard];
        if (!count) {
            clipboard.items = @[];
            return NG_SUCCESS;
        }
        NSMutableDictionary<NSString*, id>* item = [NSMutableDictionary dictionary];
        for (size_t i = 0; i < count; ++i) {
            NSString* type = entries[i].format ? ng_ios_pasteboard_type(entries[i].format) : nil;
            if (!type || (entries[i].len && !entries[i].data) || item[type]) return NG_ERROR_INVALID_PARAMETER;
            item[type] = [NSData dataWithBytes:entries[i].data length:entries[i].len];
        }
        clipboard.items = @[ item ];
        return NG_SUCCESS;
    }
}

void ng_platform_clipboard_free(unsigned char* data) {
    free(data);
}
