#ifndef AUREA_COMMON_SHORTCUTS_H
#define AUREA_COMMON_SHORTCUTS_H

/* Keys a window keeps for itself even when a child view (an embedded web
 * view, say) has keyboard focus. Keyed by the platform window handle, used
 * on the UI thread only. Key codes and modifiers are Aurea's (input.h). */

/* Replaces the window's shortcuts. count 0 clears them. */
int ng_shortcuts_set(void* window, const unsigned int* keycodes, const unsigned int* modifiers, int count);

/* 1 if key with exactly these modifiers is one of the window's shortcuts. */
int ng_shortcuts_match(void* window, unsigned int keycode, unsigned int modifiers);

/* Forgets the window. On destroy. */
void ng_shortcuts_clear(void* window);

#endif
