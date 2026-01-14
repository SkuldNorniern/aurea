#include "native_gui.h"
#include <stddef.h>

#ifdef _WIN32
#include "platform/windows.h"
#elif defined(__APPLE__)
#include "platform/macos.h"
#else
#include "platform/linux.h"
#endif

int ng_add_raw_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    return ng_platform_add_menu_item(menu, title, id);
}

int ng_attach_menu_to_window(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;
    return ng_platform_attach_menu(window, menu);
}

void ng_cleanup(void) {
    ng_platform_cleanup();
}

NGMenuHandle ng_create_menu_handle(void) {
    return ng_platform_create_menu();
}

NGHandle ng_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    return ng_platform_create_window(title, width, height);
}

void ng_destroy_menu_handle(NGMenuHandle handle) {
    if (handle) {
        ng_platform_destroy_menu(handle);
    }
}

void ng_destroy_window(NGHandle handle) {
    if (handle) {
        ng_platform_destroy_window(handle);
    }
}

int ng_handle_menu_event(NGMenuHandle menu, unsigned int id) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    return ng_platform_handle_menu_event(menu, id);
}

int ng_init(void) {
    return ng_platform_init();
}

int ng_poll_events(void) {
    return ng_platform_poll_events();
} 