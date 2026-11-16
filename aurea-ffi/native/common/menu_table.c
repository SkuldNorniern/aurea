#include "menu_table.h"
#include "shortcuts.h"
#include "input.h"
#include <stdlib.h>

/* Menus are made and used on the UI thread only, so nothing here locks. */

typedef struct {
    void* menu;
    void* root;
} RootEntry;

typedef struct {
    void* root;
    void* menu;
    unsigned int keycode;
    unsigned int modifiers;
    unsigned int id;
} ShortcutEntry;

typedef struct {
    void* window;
    void* root;
} WindowEntry;

/* A growable list of `size`-byte entries. */
typedef struct {
    void* items;
    int count;
    int cap;
} List;

static List g_roots = {0};
static List g_shortcuts = {0};
static List g_windows = {0};

static void* list_push(List* list, size_t size) {
    if (list->count == list->cap) {
        int cap = list->cap ? list->cap * 2 : 16;
        void* grown = realloc(list->items, (size_t)cap * size);
        if (!grown) return NULL;
        list->items = grown;
        list->cap = cap;
    }
    return (char*)list->items + (size_t)list->count++ * size;
}

#define ROOTS ((RootEntry*)g_roots.items)
#define SHORTCUTS ((ShortcutEntry*)g_shortcuts.items)
#define WINDOWS ((WindowEntry*)g_windows.items)

void ng_menu_table_set_root(void* menu, void* root) {
    if (!menu) return;
    for (int i = 0; i < g_roots.count; i++) {
        if (ROOTS[i].menu == menu) {
            ROOTS[i].root = root;
            return;
        }
    }
    RootEntry* entry = (RootEntry*)list_push(&g_roots, sizeof(RootEntry));
    if (!entry) return;
    entry->menu = menu;
    entry->root = root;
}

void* ng_menu_table_root(void* menu) {
    for (int i = 0; i < g_roots.count; i++) {
        if (ROOTS[i].menu == menu) return ROOTS[i].root;
    }
    return menu;
}

void ng_menu_table_add_shortcut(void* menu, const char* title, unsigned int id) {
    unsigned int keycode = NG_KEY_UNKNOWN;
    unsigned int modifiers = 0;
    if (!menu || !title || !ng_shortcut_parse(title, &keycode, &modifiers)) return;
    ShortcutEntry* entry = (ShortcutEntry*)list_push(&g_shortcuts, sizeof(ShortcutEntry));
    if (!entry) return;
    entry->root = ng_menu_table_root(menu);
    entry->menu = menu;
    entry->keycode = keycode;
    entry->modifiers = modifiers;
    entry->id = id;
}

int ng_menu_table_next_shortcut(void* root, unsigned int keycode, unsigned int modifiers, int* at,
                                void** menu, unsigned int* id) {
    for (int i = *at; i < g_shortcuts.count; i++) {
        ShortcutEntry* s = &SHORTCUTS[i];
        if (s->root != root || s->keycode != keycode || s->modifiers != modifiers) continue;
        *at = i + 1;
        *menu = s->menu;
        *id = s->id;
        return 1;
    }
    *at = g_shortcuts.count;
    return 0;
}

void ng_menu_table_forget_item(unsigned int id) {
    int kept = 0;
    for (int i = 0; i < g_shortcuts.count; i++) {
        if (SHORTCUTS[i].id != id) SHORTCUTS[kept++] = SHORTCUTS[i];
    }
    g_shortcuts.count = kept;
}

void ng_menu_table_forget_menu(void* menu) {
    int kept = 0;
    for (int i = 0; i < g_shortcuts.count; i++) {
        if (SHORTCUTS[i].menu != menu && SHORTCUTS[i].root != menu) SHORTCUTS[kept++] = SHORTCUTS[i];
    }
    g_shortcuts.count = kept;
    kept = 0;
    for (int i = 0; i < g_roots.count; i++) {
        if (ROOTS[i].menu != menu) ROOTS[kept++] = ROOTS[i];
    }
    g_roots.count = kept;
    kept = 0;
    for (int i = 0; i < g_windows.count; i++) {
        if (WINDOWS[i].root != menu) WINDOWS[kept++] = WINDOWS[i];
    }
    g_windows.count = kept;
}

void ng_menu_table_bind_window(void* window, void* root) {
    if (!window) return;
    for (int i = 0; i < g_windows.count; i++) {
        if (WINDOWS[i].window == window) {
            WINDOWS[i].root = root;
            return;
        }
    }
    WindowEntry* entry = (WindowEntry*)list_push(&g_windows, sizeof(WindowEntry));
    if (!entry) return;
    entry->window = window;
    entry->root = root;
}

void* ng_menu_table_window_root(void* window) {
    for (int i = 0; i < g_windows.count; i++) {
        if (WINDOWS[i].window == window) return WINDOWS[i].root;
    }
    return NULL;
}

void ng_menu_table_forget_window(void* window) {
    int kept = 0;
    for (int i = 0; i < g_windows.count; i++) {
        if (WINDOWS[i].window != window) WINDOWS[kept++] = WINDOWS[i];
    }
    g_windows.count = kept;
}
