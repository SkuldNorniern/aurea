#ifndef AUREA_COMMON_SHORTCUTS_H
#define AUREA_COMMON_SHORTCUTS_H

/* Replaces the window's shortcuts. */
int ng_shortcuts_set(void* window, const unsigned int* keycodes, const unsigned int* modifiers, int count);

/* 1 if key with exactly these modifiers is one of the window's shortcuts. */
int ng_shortcuts_match(void* window, unsigned int keycode, unsigned int modifiers);

/* Forgets the window. */
void ng_shortcuts_clear(void* window);

/* Reads the shortcut after the tab in a menu title such as "Save\tCtrl+S"; 0 when it has none. */
int ng_shortcut_parse(const char* title, unsigned int* keycode, unsigned int* modifiers);

#endif
