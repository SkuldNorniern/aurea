#include "menu.h"
#include "common/errors.h"
#include <windows.h>

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

NGMenuHandle ng_windows_create_submenu(NGMenuHandle parent_menu, const char* title) {
    if (!parent_menu || !title) return NULL;
    
    HMENU submenu = CreatePopupMenu();
    if (!submenu) return NULL;
    
    if (!AppendMenuA((HMENU)parent_menu, MF_STRING | MF_POPUP, (UINT_PTR)submenu, title)) {
        DestroyMenu(submenu);
        return NULL;
    }
    
    return (NGMenuHandle)submenu;
}

int ng_windows_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    
    // Windows uses command IDs for menu items, starting from 1
    UINT command_id = id + 1;
    
    if (!AppendMenuA((HMENU)menu, MF_STRING, command_id, title)) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    return NG_SUCCESS;
}

