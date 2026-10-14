#include "menu.h"
#include "utils.h"
#include "common/errors.h"
#include "common/input.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <ctype.h>
#include <stdlib.h>
#include <string.h>
#include <wctype.h>

typedef struct {
    HMENU menu;
    HMENU root;
} MenuRootEntry;

typedef struct {
    HMENU root;
    unsigned int keycode;
    unsigned int modifiers;
    unsigned int id;
} MenuShortcutEntry;

static MenuRootEntry g_menu_roots[256] = {0};
static int g_menu_root_count = 0;
static MenuShortcutEntry g_shortcuts[512] = {0};
static int g_shortcut_count = 0;

static void forget_menu_root(HMENU menu) {
    int kept = 0;
    for (int i = 0; i < g_menu_root_count; i++) {
        if (g_menu_roots[i].menu != menu) g_menu_roots[kept++] = g_menu_roots[i];
    }
    g_menu_root_count = kept;
}

static void register_menu_root(HMENU menu, HMENU root) {
    if (!menu) return;
    for (int i = 0; i < g_menu_root_count; i++) {
        if (g_menu_roots[i].menu == menu) {
            g_menu_roots[i].root = root;
            return;
        }
    }
    if (g_menu_root_count < 256) {
        g_menu_roots[g_menu_root_count].menu = menu;
        g_menu_roots[g_menu_root_count].root = root;
        g_menu_root_count++;
    }
}

static HMENU find_menu_root(HMENU menu) {
    for (int i = 0; i < g_menu_root_count; i++) {
        if (g_menu_roots[i].menu == menu) {
            return g_menu_roots[i].root;
        }
    }
    return menu;
}

static int token_eq(const char* a, const char* b) {
    while (*a && *b) {
        if (tolower((unsigned char)*a) != tolower((unsigned char)*b)) {
            return 0;
        }
        a++;
        b++;
    }
    return *a == '\0' && *b == '\0';
}

static int shortcut_keycode(const char* token, unsigned int* keycode) {
    size_t len = strlen(token);
    if (len == 1) {
        char c = (char)toupper((unsigned char)token[0]);
        if (c >= 'A' && c <= 'Z') {
            *keycode = NG_KEY_A + (unsigned int)(c - 'A');
            return 1;
        }
        if (c >= '0' && c <= '9') {
            *keycode = NG_KEY_0 + (unsigned int)(c - '0');
            return 1;
        }
    }

    if (token_eq(token, "Enter")) { *keycode = NG_KEY_ENTER; return 1; }
    if (token_eq(token, "Tab")) { *keycode = NG_KEY_TAB; return 1; }
    if (token_eq(token, "Esc") || token_eq(token, "Escape")) { *keycode = NG_KEY_ESCAPE; return 1; }
    if (token_eq(token, "Space")) { *keycode = NG_KEY_SPACE; return 1; }
    if (token_eq(token, "Backspace")) { *keycode = NG_KEY_BACKSPACE; return 1; }
    if (token_eq(token, "Delete")) { *keycode = NG_KEY_DELETE; return 1; }

    if ((token[0] == 'F' || token[0] == 'f') && len <= 3) {
        int n = atoi(token + 1);
        if (n >= 1 && n <= 12) {
            *keycode = NG_KEY_F1 + (unsigned int)(n - 1);
            return 1;
        }
    }

    return 0;
}

static void register_shortcut(HMENU menu, const char* title, unsigned int id) {
    const char* tab = strchr(title, '\t');
    if (!tab || tab[1] == '\0') return;

    unsigned int mods = 0;
    unsigned int keycode = NG_KEY_UNKNOWN;
    char buf[128];
    strncpy(buf, tab + 1, sizeof(buf) - 1);
    buf[sizeof(buf) - 1] = '\0';

    char* part = strtok(buf, "+");
    while (part) {
        while (*part == ' ') part++;
        if (token_eq(part, "Ctrl") || token_eq(part, "Control")) {
            mods |= NG_MOD_CTRL;
        } else if (token_eq(part, "Shift")) {
            mods |= NG_MOD_SHIFT;
        } else if (token_eq(part, "Alt") || token_eq(part, "Option")) {
            mods |= NG_MOD_ALT;
        } else if (token_eq(part, "Cmd") || token_eq(part, "Command") || token_eq(part, "Meta")) {
            mods |= NG_MOD_META;
        } else {
            shortcut_keycode(part, &keycode);
        }
        part = strtok(NULL, "+");
    }

    if (keycode == NG_KEY_UNKNOWN || g_shortcut_count >= 512) return;

    g_shortcuts[g_shortcut_count].root = find_menu_root(menu);
    g_shortcuts[g_shortcut_count].keycode = keycode;
    g_shortcuts[g_shortcut_count].modifiers = mods;
    g_shortcuts[g_shortcut_count].id = id;
    g_shortcut_count++;
}

NGMenuHandle ng_windows_create_menu(void) {
    HMENU menubar = CreateMenu();
    register_menu_root(menubar, menubar);
    return (NGMenuHandle)menubar;
}

void ng_windows_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    ng_windows_menu_clear(handle);
    forget_menu_root((HMENU)handle);
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
    
    register_menu_root(submenu, find_menu_root((HMENU)parent_menu));
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
    register_shortcut((HMENU)menu, title, id);
    return NG_SUCCESS;
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

    HMENU root = find_menu_root(menu);
    for (int i = 0; i < g_shortcut_count; i++) {
        if (g_shortcuts[i].root != root) continue;
        if (g_shortcuts[i].keycode != keycode) continue;
        if (g_shortcuts[i].modifiers != modifiers) continue;
        UINT state = GetMenuState(menu, g_shortcuts[i].id + 1, MF_BYCOMMAND);
        if (state == (UINT)-1 || (state & (MF_GRAYED | MF_DISABLED))) continue;
        return (int)g_shortcuts[i].id;
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

static void forget_shortcuts(unsigned int id) {
    int kept = 0;
    for (int i = 0; i < g_shortcut_count; i++) {
        if (g_shortcuts[i].id != id) g_shortcuts[kept++] = g_shortcuts[i];
    }
    g_shortcut_count = kept;
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
    forget_shortcuts(id);
    register_shortcut((HMENU)menu, title, id);
    return NG_SUCCESS;
}

int ng_windows_menu_item_remove(NGMenuHandle menu, unsigned int id) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    if (!DeleteMenu((HMENU)menu, id + 1, MF_BYCOMMAND)) return NG_ERROR_INVALID_PARAMETER;
    forget_shortcuts(id);
    return NG_SUCCESS;
}

int ng_windows_menu_clear(NGMenuHandle menu) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    for (int i = GetMenuItemCount((HMENU)menu) - 1; i >= 0; i--) {
        HMENU child = GetSubMenu((HMENU)menu, i);
        if (child) {
            ng_windows_menu_clear((NGMenuHandle)child);
            forget_menu_root(child);
        }
        UINT command = GetMenuItemID((HMENU)menu, i);
        if (command != (UINT)-1 && command > 0) forget_shortcuts(command - 1);
        DeleteMenu((HMENU)menu, (UINT)i, MF_BYPOSITION);
    }
    return NG_SUCCESS;
}
