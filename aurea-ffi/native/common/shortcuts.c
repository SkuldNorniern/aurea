#include "shortcuts.h"
#include "errors.h"
#include "input.h"
#include <ctype.h>
#include <stdlib.h>
#include <string.h>
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

static int token_eq(const char* a, const char* b) {
    while (*a && *b) {
        if (tolower((unsigned char)*a) != tolower((unsigned char)*b)) return 0;
        a++;
        b++;
    }
    return *a == '\0' && *b == '\0';
}

static const struct {
    const char* name;
    unsigned int key;
} NAMED_KEYS[] = {
    {"Enter", NG_KEY_ENTER}, {"Return", NG_KEY_ENTER}, {"Tab", NG_KEY_TAB},
    {"Esc", NG_KEY_ESCAPE}, {"Escape", NG_KEY_ESCAPE}, {"Space", NG_KEY_SPACE},
    {"Backspace", NG_KEY_BACKSPACE}, {"Delete", NG_KEY_DELETE}, {"Del", NG_KEY_DELETE},
    {"Insert", NG_KEY_INSERT}, {"Ins", NG_KEY_INSERT}, {"Home", NG_KEY_HOME}, {"End", NG_KEY_END},
    {"PageUp", NG_KEY_PAGE_UP}, {"PgUp", NG_KEY_PAGE_UP}, {"PageDown", NG_KEY_PAGE_DOWN},
    {"PgDn", NG_KEY_PAGE_DOWN}, {"Up", NG_KEY_UP}, {"Down", NG_KEY_DOWN}, {"Left", NG_KEY_LEFT},
    {"Right", NG_KEY_RIGHT}, {"Plus", NG_KEY_EQUALS}, {"Minus", NG_KEY_MINUS},
};

static const char PUNCTUATION[] = "-=[]\\;'`,./";
static const unsigned int PUNCTUATION_KEYS[] = {
    NG_KEY_MINUS, NG_KEY_EQUALS, NG_KEY_LEFT_BRACKET, NG_KEY_RIGHT_BRACKET, NG_KEY_BACKSLASH,
    NG_KEY_SEMICOLON, NG_KEY_APOSTROPHE, NG_KEY_GRAVE, NG_KEY_COMMA, NG_KEY_PERIOD, NG_KEY_SLASH,
};

static unsigned int shortcut_key(const char* token) {
    size_t len = strlen(token);
    if (len == 1) {
        char c = (char)toupper((unsigned char)token[0]);
        if (c >= 'A' && c <= 'Z') return NG_KEY_A + (unsigned int)(c - 'A');
        if (c >= '0' && c <= '9') return NG_KEY_0 + (unsigned int)(c - '0');
        if (c == '+') return NG_KEY_EQUALS;
        const char* at = strchr(PUNCTUATION, c);
        if (at && c) return PUNCTUATION_KEYS[at - PUNCTUATION];
    }
    for (size_t i = 0; i < sizeof(NAMED_KEYS) / sizeof(NAMED_KEYS[0]); i++) {
        if (token_eq(token, NAMED_KEYS[i].name)) return NAMED_KEYS[i].key;
    }
    if ((token[0] == 'F' || token[0] == 'f') && len >= 2 && len <= 3) {
        int n = atoi(token + 1);
        if (n >= 1 && n <= 12) return NG_KEY_F1 + (unsigned int)(n - 1);
    }
    return NG_KEY_UNKNOWN;
}

int ng_shortcut_parse(const char* title, unsigned int* keycode, unsigned int* modifiers) {
    const char* tab = title ? strchr(title, '\t') : NULL;
    if (!tab || !tab[1] || !keycode || !modifiers) return 0;
    char buf[128];
    size_t len = strlen(tab + 1);
    if (len >= sizeof(buf)) return 0;
    memcpy(buf, tab + 1, len + 1);

    unsigned int mods = 0;
    unsigned int key = NG_KEY_UNKNOWN;
    char* part = buf;
    while (*part) {
        char* plus = strchr(part + 1, '+');
        if (plus) *plus = '\0';
        while (*part == ' ') part++;
        char* end = part + strlen(part);
        while (end > part && end[-1] == ' ') *--end = '\0';
        if (token_eq(part, "Ctrl") || token_eq(part, "Control") || token_eq(part, "Cmd") ||
            token_eq(part, "Command")) {
            mods |= NG_MOD_CTRL;
        } else if (token_eq(part, "Shift")) {
            mods |= NG_MOD_SHIFT;
        } else if (token_eq(part, "Alt") || token_eq(part, "Option")) {
            mods |= NG_MOD_ALT;
        } else if (token_eq(part, "Meta") || token_eq(part, "Win") || token_eq(part, "Super")) {
            mods |= NG_MOD_META;
        } else if (*part) {
            key = shortcut_key(part);
        }
        if (!plus) break;
        part = plus + 1;
    }
    if (key == NG_KEY_UNKNOWN) return 0;
    *keycode = key;
    *modifiers = mods;
    return 1;
}
