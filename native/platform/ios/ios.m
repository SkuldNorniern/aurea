#import "ios.h"
#import "window.h"
#import "elements.h"
#import "../../common/errors.h"
#import <UIKit/UIKit.h>

int ng_ios_init(void) {
    // iOS initialization is handled by app delegate
    return NG_SUCCESS;
}

void ng_ios_cleanup(void) {
    // iOS cleanup is handled by app lifecycle
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

// Stub implementations for platform invalidation functions
// These are called by Rust code but iOS handles invalidation differently
void ng_platform_button_invalidate(NGHandle handle) {
    // iOS buttons invalidate automatically, stub for compatibility
}

void ng_platform_label_invalidate(NGHandle handle) {
    // iOS labels invalidate automatically, stub for compatibility
}

void ng_platform_canvas_invalidate(NGHandle handle) {
    // Canvas invalidation is handled by the view system
}

void ng_platform_text_editor_invalidate(NGHandle handle) {
    // Text editor invalidation handled by UIKit
}

void ng_platform_text_view_invalidate(NGHandle handle) {
    // Text view invalidation handled by UIKit
}

void ng_platform_progress_bar_invalidate(NGHandle handle) {
    // Progress bar invalidation handled by UIKit
}

