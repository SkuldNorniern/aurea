#import "ios.h"
#import "window.h"
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

NGHandle ng_ios_create_button(const char* title) {
    // TODO: Create iOS button
    return NULL;
}

NGHandle ng_ios_create_label(const char* text) {
    // TODO: Create iOS label
    return NULL;
}

NGHandle ng_ios_create_canvas(int width, int height) {
    // TODO: Create iOS canvas/view for rendering
    return NULL;
}

