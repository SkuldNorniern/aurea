#include "shortcuts.h"
#include "errors.h"
#include <stddef.h>

#define AUREA_SHORTCUT_WINDOWS 32
#define AUREA_SHORTCUTS_PER_WINDOW 64

typedef struct {
    void* window;
    int count;
    unsigned int keycodes[AUREA_SHORTCUTS_PER_WINDOW];
    unsigned int modifiers[AUREA_SHORTCUTS_PER_WINDOW];
} NGShortcutTable;

static NGShortcutTable g_tables[AUREA_SHORTCUT_WINDOWS];

static NGShortcutTable* ng_shortcuts_find(void* window) {
    if (!window) return NULL;
    for (int i = 0; i < AUREA_SHORTCUT_WINDOWS; i++) {
        if (g_tables[i].window == window) return &g_tables[i];
    }
    return NULL;
}

int ng_shortcuts_set(void* window, const unsigned int* keycodes, const unsigned int* modifiers, int count) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    if (count < 0 || count > AUREA_SHORTCUTS_PER_WINDOW) return NG_ERROR_INVALID_PARAMETER;
    if (count > 0 && (!keycodes || !modifiers)) return NG_ERROR_INVALID_PARAMETER;

    NGShortcutTable* table = ng_shortcuts_find(window);
    if (count == 0) {
        if (table) table->window = NULL;
        return NG_SUCCESS;
    }
    if (!table) {
        for (int i = 0; i < AUREA_SHORTCUT_WINDOWS && !table; i++) {
            if (!g_tables[i].window) table = &g_tables[i];
        }
    }
    if (!table) return NG_ERROR_PLATFORM_SPECIFIC;

    table->window = window;
    table->count = count;
    for (int i = 0; i < count; i++) {
        table->keycodes[i] = keycodes[i];
        table->modifiers[i] = modifiers[i];
    }
    return NG_SUCCESS;
}

int ng_shortcuts_match(void* window, unsigned int keycode, unsigned int modifiers) {
    NGShortcutTable* table = ng_shortcuts_find(window);
    if (!table) return 0;
    for (int i = 0; i < table->count; i++) {
        if (table->keycodes[i] == keycode && table->modifiers[i] == modifiers) return 1;
    }
    return 0;
}

void ng_shortcuts_clear(void* window) {
    NGShortcutTable* table = ng_shortcuts_find(window);
    if (table) table->window = NULL;
}
