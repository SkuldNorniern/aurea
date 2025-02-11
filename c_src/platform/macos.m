#import "macos.h"
#import "macos/window.h"
#import "macos/menu.h"
#import "../common/errors.h"
#import "macos/elements.h"


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

NGHandle ng_platform_create_button(const char* title) {
    return ng_macos_create_button(title);
}

NGHandle ng_platform_create_label(const char* text) {
    return ng_macos_create_label(text);
}

NGHandle ng_platform_create_box(int is_vertical) {
    return ng_macos_create_box(is_vertical);
}

int ng_platform_box_add(NGHandle box, NGHandle element) {
    return ng_macos_box_add(box, element);
}

int ng_platform_set_window_content(NGHandle window, NGHandle content) {
    return ng_macos_set_window_content(window, content);
}

NGHandle ng_platform_create_text_editor(void) {
    return ng_macos_create_text_editor();
}

NGHandle ng_platform_create_text_view(int is_editable) {
    return ng_macos_create_text_view(is_editable);
}

int ng_platform_set_text_content(NGHandle text_handle, const char* content) {
    return ng_macos_set_text_content(text_handle, content);
}

char* ng_platform_get_text_content(NGHandle text_handle) {
    return ng_macos_get_text_content(text_handle);
}

void ng_platform_free_text_content(char* content) {
    ng_macos_free_text_content(content);
} 