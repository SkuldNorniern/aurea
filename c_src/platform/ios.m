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

