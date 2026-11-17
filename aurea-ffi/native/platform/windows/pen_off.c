#include "pen.h"

void ng_windows_pen_message(HWND root, UINT msg, WPARAM wParam) {
    (void)root;
    (void)msg;
    (void)wParam;
}

void ng_windows_pen_capture_lost(HWND root, WPARAM wParam) {
    (void)root;
    (void)wParam;
}

int ng_windows_pen_mode(void) {
    return AUREA_PEN_MODE_AUTO;
}

void ng_windows_pen_set_mode(int mode) {
    (void)mode;
}

int ng_windows_pen_ink_live(void) {
    return 0;
}

unsigned int ng_windows_pen_bits(void) {
    return 0;
}

int ng_windows_pen_query_gestures(LRESULT* status) {
    (void)status;
    return 0;
}

void ng_windows_wintab_activate(HWND root, int active) {
    (void)root;
    (void)active;
}

void ng_windows_wintab_destroyed(HWND root) {
    (void)root;
}

void ng_windows_wintab_sync(void) {
}

void ng_windows_wintab_yield(void) {
}

void ng_windows_wintab_message(HWND root, UINT msg, LPARAM lParam) {
    (void)root;
    (void)msg;
    (void)lParam;
}
