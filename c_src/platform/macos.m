#import "macos.h"
#import "macos/window.h"
#import "macos/menu.h"
#import "../common/errors.h"
#import <Cocoa/Cocoa.h>

static BOOL app_initialized = FALSE;

int ng_platform_init(void) {
    if (!app_initialized) {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        [NSApp finishLaunching];
        app_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_platform_cleanup(void) {
    if (app_initialized) {
        app_initialized = FALSE;
    }
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    return ng_macos_create_window(title, width, height);
}

void ng_platform_destroy_window(NGHandle handle) {
    ng_macos_destroy_window(handle);
}

NGMenuHandle ng_platform_create_menu(void) {
    return ng_macos_create_menu();
}

void ng_platform_destroy_menu(NGMenuHandle handle) {
    ng_macos_destroy_menu(handle);
}

int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu) {
    return ng_macos_attach_menu(window, menu);
}

int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    return ng_macos_add_menu_item(menu, title, id);
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle parentMenu, const char* title) {
    return ng_macos_create_submenu(parentMenu, title);
}

int ng_platform_run(void) {
    [NSApp run];
    return NG_SUCCESS;
} 