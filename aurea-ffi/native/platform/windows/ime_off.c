#include "ime.h"
#include "common/errors.h"

int ng_windows_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height) {
    (void)window;
    (void)x;
    (void)y;
    (void)width;
    (void)height;
    return NG_SUCCESS;
}

int ng_windows_window_set_ime_allowed(NGHandle window, int allowed) {
    (void)window;
    (void)allowed;
    return NG_SUCCESS;
}

int ng_windows_ime_message(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam, LRESULT* result) {
    (void)hwnd;
    (void)msg;
    (void)wParam;
    (void)lParam;
    (void)result;
    return 0;
}

void ng_windows_ime_complete(HWND hwnd) {
    (void)hwnd;
}
