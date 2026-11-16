#ifndef NATIVE_GUI_WINDOWS_WINDOW_STATE_H
#define NATIVE_GUI_WINDOWS_WINDOW_STATE_H

#include <windows.h>
#include "common/platform_api.h"

#ifdef __cplusplus
extern "C" {
#endif

/* What Aurea keeps for one of its top-level windows, from WM_NCCREATE to WM_NCDESTROY. */
typedef struct NGWindow {
    HWND hwnd;
    struct NGWindow* next;

    ScaleFactorCallback on_scale;
    /* Rust hears this window's lifecycle events. */
    BOOL lifecycle;
    BOOL minimized;
    HMONITOR monitor;
    /* -1 until first looked at. */
    int occlusion;

    BOOL pointer_inside;
    /* A TrackMouseEvent leave is armed. */
    BOOL tracking_leave;

    /* The first half of a surrogate pair, until the second arrives. */
    wchar_t high_surrogate;
    /* An Alt key the menu bar takes, held until its key up. */
    WPARAM menu_access_key;
    /* A key a menu shortcut took, so its key up is swallowed too. */
    WPARAM menu_took_key;
    BOOL shortcut_took_syskey;

    BOOL close_guard;
    BOOL hide_on_close;
    BOOL maximize_on_show;
    /* Logical minimum client size; 0 is unset. */
    int min_client_w;
    int min_client_h;
    HWND content;

    HICON icon;
    HICON small_icon;
    /* Shown over the content; NULL is the arrow. */
    HCURSOR cursor;
    /* A cursor made from pixels, destroyed when replaced. */
    HCURSOR owned_cursor;
    int grab_mode;

    /* The IME window's place, in physical pixels, once the app sets it. */
    BOOL ime_area_set;
    RECT ime_area;
    /* The app has heard the preedit cleared. */
    BOOL ime_cleared;
} NGWindow;

/* Makes and frees `hwnd`'s state; the window procedure calls these. */
NGWindow* ng_windows_window_state_create(HWND hwnd);
void ng_windows_window_state_free(HWND hwnd);
/* `hwnd`'s state, or NULL for a window that is not one of Aurea's top-level windows. */
NGWindow* ng_windows_window_state(HWND hwnd);
/* The first of every live window's state, in no set order; follow `next`. */
NGWindow* ng_windows_windows(void);
/* The Aurea window `hwnd` sits in, itself included; NULL outside one. */
HWND ng_windows_root_window(HWND hwnd);

#ifdef __cplusplus
}
#endif

#endif
