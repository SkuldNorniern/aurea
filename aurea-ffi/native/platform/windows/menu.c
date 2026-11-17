#include "menu.h"
#include "utils.h"
#include "window_state.h"
#include "common/shortcuts.h"
#include "common/menu_table.h"
#include "common/errors.h"
#include "common/input.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <ctype.h>
#include <stdlib.h>
#include <string.h>
#include <wctype.h>

NGMenuHandle ng_windows_create_menu(void) {
    HMENU menubar = CreateMenu();
    ng_menu_table_set_root(menubar, menubar);
    return (NGMenuHandle)menubar;
}

void ng_windows_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    ng_windows_menu_clear(handle);
    ng_menu_table_forget_menu(handle);
    DestroyMenu((HMENU)handle);
}

int ng_windows_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;
    
    if (!SetMenu((HWND)window, (HMENU)menu)) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    DrawMenuBar((HWND)window);
    ng_windows_fit_content((HWND)window);
    return NG_SUCCESS;
}

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
    wchar_t* wide = ng_windows_utf8_to_wide(label);
    if (!wide) {
        DestroyMenu(submenu);
        return NULL;
    }
    BOOL added = AppendMenuW((HMENU)parent_menu, MF_STRING | MF_POPUP, (UINT_PTR)submenu, wide);
    free(wide);
    if (!added) {
        DestroyMenu(submenu);
        return NULL;
    }
    
    ng_menu_table_set_root(submenu, ng_menu_table_root(parent_menu));
    return (NGMenuHandle)submenu;
}

int ng_windows_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;

    UINT command_id = id + 1;

    wchar_t* wide = ng_windows_utf8_to_wide(title);
    if (!wide) return NG_ERROR_PLATFORM_SPECIFIC;
    BOOL added = AppendMenuW((HMENU)menu, MF_STRING, command_id, wide);
    free(wide);
    if (!added) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    ng_menu_table_add_shortcut(menu, title, id);
    return NG_SUCCESS;
}

int ng_windows_menu_move_last(NGMenuHandle menu, unsigned int index) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    HMENU handle = (HMENU)menu;
    int count = GetMenuItemCount(handle);
    if (count <= 0) return NG_ERROR_PLATFORM_SPECIFIC;
    if (index >= (unsigned int)(count - 1)) return NG_SUCCESS;
    wchar_t text[512];
    MENUITEMINFOW info = {0};
    info.cbSize = sizeof(info);
    info.fMask = MIIM_ID | MIIM_STATE | MIIM_FTYPE | MIIM_STRING | MIIM_SUBMENU | MIIM_DATA;
    info.dwTypeData = text;
    info.cch = sizeof(text) / sizeof(text[0]);
    if (!GetMenuItemInfoW(handle, (UINT)(count - 1), TRUE, &info)) return NG_ERROR_PLATFORM_SPECIFIC;
    /* RemoveMenu keeps a submenu alive for the row put back below. */
    if (!RemoveMenu(handle, (UINT)(count - 1), MF_BYPOSITION)) return NG_ERROR_PLATFORM_SPECIFIC;
    info.cch = 0;
    return InsertMenuItemW(handle, index, TRUE, &info) ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_windows_add_menu_separator(NGMenuHandle menu) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;

    if (!AppendMenuW((HMENU)menu, MF_SEPARATOR, 0, NULL)) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }

    return NG_SUCCESS;
}

static int title_access_key(const wchar_t* text, wchar_t* key) {
    for (const wchar_t* p = text; *p; p++) {
        if (*p != L'&') continue;
        if (p[1] == L'&') {
            p++;
            continue;
        }
        if (!p[1]) return 0;
        *key = p[1];
        return 1;
    }
    return 0;
}

int ng_windows_menu_takes_key(NGHandle window, unsigned int key) {
    if (!window) return 0;
    HMENU bar = GetMenu((HWND)window);
    if (!bar) return 0;
    if (!((key >= 'A' && key <= 'Z') || (key >= '0' && key <= '9'))) return 0;

    int count = GetMenuItemCount(bar);
    for (int i = 0; i < count; i++) {
        wchar_t text[256];
        if (GetMenuStringW(bar, (UINT)i, text, 256, MF_BYPOSITION) <= 0) continue;
        wchar_t access = 0;
        if (title_access_key(text, &access) && (unsigned int)towupper(access) == key) return 1;
    }
    return 0;
}

static int shortcut_item(NGHandle window, unsigned int keycode, unsigned int modifiers) {
    if (!window) return -1;
    HMENU menu = GetMenu((HWND)window);
    if (!menu) return -1;

    void* root = ng_menu_table_root(menu);
    void* owner;
    unsigned int id;
    for (int at = 0; ng_menu_table_next_shortcut(root, keycode, modifiers, &at, &owner, &id);) {
        UINT state = GetMenuState(menu, id + 1, MF_BYCOMMAND);
        if (state == (UINT)-1 || (state & (MF_GRAYED | MF_DISABLED))) continue;
        return (int)id;
    }
    return -1;
}

int ng_windows_menu_has_shortcut(NGHandle window, unsigned int keycode, unsigned int modifiers) {
    return shortcut_item(window, keycode, modifiers) >= 0;
}

int ng_windows_handle_menu_shortcut(NGHandle window, unsigned int keycode, unsigned int modifiers) {
    int id = shortcut_item(window, keycode, modifiers);
    if (id < 0) return 0;
    ng_invoke_menu_callback((unsigned int)id);
    return 1;
}

int ng_windows_add_menu_check_item(NGMenuHandle menu, const char* title, unsigned int id, int checked, int radio) {
    int result = ng_windows_add_menu_item(menu, title, id);
    if (result != NG_SUCCESS) return result;
    MENUITEMINFOW info = {0};
    info.cbSize = sizeof(info);
    info.fMask = MIIM_FTYPE | MIIM_STATE;
    if (!GetMenuItemInfoW((HMENU)menu, id + 1, FALSE, &info)) return NG_ERROR_PLATFORM_SPECIFIC;
    if (radio) info.fType |= MFT_RADIOCHECK;
    info.fState = checked ? MFS_CHECKED : MFS_UNCHECKED;
    return SetMenuItemInfoW((HMENU)menu, id + 1, FALSE, &info) ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_windows_menu_item_set_enabled(NGMenuHandle menu, unsigned int id, int enabled) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    UINT flags = MF_BYCOMMAND | (enabled ? MF_ENABLED : MF_GRAYED);
    return EnableMenuItem((HMENU)menu, id + 1, flags) == (BOOL)-1 ? NG_ERROR_INVALID_PARAMETER : NG_SUCCESS;
}

int ng_windows_menu_item_set_checked(NGMenuHandle menu, unsigned int id, int checked) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    UINT flags = MF_BYCOMMAND | (checked ? MF_CHECKED : MF_UNCHECKED);
    return CheckMenuItem((HMENU)menu, id + 1, flags) == (DWORD)-1 ? NG_ERROR_INVALID_PARAMETER : NG_SUCCESS;
}

int ng_windows_menu_item_set_title(NGMenuHandle menu, unsigned int id, const char* title) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    wchar_t* wide = ng_windows_utf8_to_wide(title);
    if (!wide) return NG_ERROR_PLATFORM_SPECIFIC;
    MENUITEMINFOW info = {0};
    info.cbSize = sizeof(info);
    info.fMask = MIIM_STRING;
    info.dwTypeData = wide;
    BOOL ok = SetMenuItemInfoW((HMENU)menu, id + 1, FALSE, &info);
    free(wide);
    if (!ok) return NG_ERROR_INVALID_PARAMETER;
    ng_menu_table_forget_item(id);
    ng_menu_table_add_shortcut(menu, title, id);
    return NG_SUCCESS;
}

int ng_windows_menu_item_remove(NGMenuHandle menu, unsigned int id) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    if (!DeleteMenu((HMENU)menu, id + 1, MF_BYCOMMAND)) return NG_ERROR_INVALID_PARAMETER;
    ng_menu_table_forget_item(id);
    return NG_SUCCESS;
}

int ng_windows_menu_clear(NGMenuHandle menu) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    for (int i = GetMenuItemCount((HMENU)menu) - 1; i >= 0; i--) {
        HMENU child = GetSubMenu((HMENU)menu, i);
        if (child) {
            ng_windows_menu_clear((NGMenuHandle)child);
            ng_menu_table_forget_menu(child);
        }
        UINT command = GetMenuItemID((HMENU)menu, i);
        if (command != (UINT)-1 && command > 0) ng_menu_table_forget_item(command - 1);
        DeleteMenu((HMENU)menu, (UINT)i, MF_BYPOSITION);
    }
    return NG_SUCCESS;
}

int ng_windows_menu_key_down(HWND hwnd, UINT msg, WPARAM wParam, unsigned int keycode, unsigned int modifiers) {
    NGWindow* w = ng_windows_window_state(hwnd);
    if (msg == WM_SYSKEYDOWN && wParam == VK_MENU && GetMenu(hwnd)) {
        return NG_MENU_KEY_DEFAULT;
    }
    if (msg == WM_SYSKEYDOWN && (modifiers & NG_MOD_ALT) && !(modifiers & NG_MOD_CTRL)
        && !ng_windows_menu_has_shortcut((void*)hwnd, keycode, modifiers)
        && ng_windows_menu_takes_key((void*)hwnd, (unsigned int)wParam)) {
        w->menu_access_key = wParam;
        return NG_MENU_KEY_DEFAULT;
    }
    w->menu_access_key = 0;
    if (ng_windows_handle_menu_shortcut((void*)hwnd, keycode, modifiers)) {
        w->shortcut_took_syskey = msg == WM_SYSKEYDOWN;
        w->menu_took_key = wParam;
        return NG_MENU_KEY_TAKEN;
    }
    return NG_MENU_KEY_PASS;
}

int ng_windows_menu_key_up(HWND hwnd, UINT msg, WPARAM wParam) {
    NGWindow* w = ng_windows_window_state(hwnd);
    if (msg == WM_SYSKEYUP && wParam == VK_MENU && GetMenu(hwnd)) {
        return NG_MENU_KEY_DEFAULT;
    }
    if (msg == WM_SYSKEYUP && w->menu_access_key != 0 && wParam == w->menu_access_key) {
        w->menu_access_key = 0;
        return NG_MENU_KEY_DEFAULT;
    }
    if (w->menu_took_key != 0 && wParam == w->menu_took_key) {
        w->menu_took_key = 0;
        return NG_MENU_KEY_TAKEN;
    }
    return NG_MENU_KEY_PASS;
}

void ng_windows_menu_command(unsigned int command_id, LPARAM lParam) {
    if (!lParam) ng_invoke_menu_callback(command_id - 1);
}
