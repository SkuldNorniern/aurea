#include "window_state.h"
#include <stdlib.h>

/* Windows run on the UI thread only, so the list needs no lock. */
static NGWindow* g_windows = NULL;
/* Tells Aurea's own windows from others that may carry user data. */
static const LONG_PTR NG_WINDOW_MARK = 0x41555245;

NGWindow* ng_windows_window_state_create(HWND hwnd) {
    NGWindow* w = (NGWindow*)calloc(1, sizeof(NGWindow));
    if (!w) return NULL;
    w->hwnd = hwnd;
    w->occlusion = -1;
    w->next = g_windows;
    g_windows = w;
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, (LONG_PTR)w);
    SetPropA(hwnd, "AureaWindow", (HANDLE)NG_WINDOW_MARK);
    return w;
}

void ng_windows_window_state_free(HWND hwnd) {
    NGWindow* w = ng_windows_window_state(hwnd);
    if (!w) return;
    for (NGWindow** at = &g_windows; *at; at = &(*at)->next) {
        if (*at == w) {
            *at = w->next;
            break;
        }
    }
    /* a window the platform closed keeps its icons until here */
    if (w->icon) DestroyIcon(w->icon);
    if (w->small_icon) DestroyIcon(w->small_icon);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    RemovePropA(hwnd, "AureaWindow");
    free(w);
}

void ng_windows_window_set_owner(HWND hwnd, unsigned long long owner) {
    NGWindow* w = ng_windows_window_state(hwnd);
    if (w) w->owner = owner;
}

int ng_windows_window_owned_by(HWND hwnd, unsigned long long owner) {
    NGWindow* w = ng_windows_window_state(hwnd);
    return w && owner && w->owner == owner;
}

NGWindow* ng_windows_window_state(HWND hwnd) {
    if (!hwnd || (LONG_PTR)GetPropA(hwnd, "AureaWindow") != NG_WINDOW_MARK) return NULL;
    return (NGWindow*)GetWindowLongPtrW(hwnd, GWLP_USERDATA);
}

NGWindow* ng_windows_windows(void) {
    return g_windows;
}

HWND ng_windows_root_window(HWND hwnd) {
    for (HWND at = hwnd; at; at = GetParent(at)) {
        if (ng_windows_window_state(at)) return at;
    }
    return NULL;
}
