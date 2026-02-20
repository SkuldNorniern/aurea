#include "menu.h"
#include "common/errors.h"
#include <windows.h>
#include <string.h>

NGMenuHandle ng_windows_create_menu(void) {
    HMENU menubar = CreateMenu();
    return (NGMenuHandle)menubar;
}

void ng_windows_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    DestroyMenu((HMENU)handle);
}

int ng_windows_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;
    
    if (!SetMenu((HWND)window, (HMENU)menu)) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    DrawMenuBar((HWND)window);
    return NG_SUCCESS;
}

/* Use display part only: "Save\tCtrl+S" -> "Save" for cleaner menu labels. */
static const char* display_title(const char* title, char* buf, size_t buf_size) {
    const char* tab = strchr(title, '\t');
    if (!tab || (size_t)(tab - title) >= buf_size) return title;
    size_t len = (size_t)(tab - title);
    memcpy(buf, title, len);
    buf[len] = '\0';
    return buf;
}

NGMenuHandle ng_windows_create_submenu(NGMenuHandle parent_menu, const char* title) {
    if (!parent_menu || !title) return NULL;
    
    HMENU submenu = CreatePopupMenu();
    if (!submenu) return NULL;
    
    char display_buf[256];
    const char* label = display_title(title, display_buf, sizeof(display_buf));
    if (!AppendMenuA((HMENU)parent_menu, MF_STRING | MF_POPUP, (UINT_PTR)submenu, label)) {
        DestroyMenu(submenu);
        return NULL;
    }
    
    return (NGMenuHandle)submenu;
}

int ng_windows_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;

    char display_buf[256];
    const char* label = display_title(title, display_buf, sizeof(display_buf));

    UINT command_id = id + 1;

    if (!AppendMenuA((HMENU)menu, MF_STRING, command_id, label)) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    return NG_SUCCESS;
}

int ng_windows_add_menu_separator(NGMenuHandle menu) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;

    if (!AppendMenuA((HMENU)menu, MF_SEPARATOR, 0, NULL)) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }

    return NG_SUCCESS;
}
