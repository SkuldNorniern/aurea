#include "menu.h"

int ng_windows_menu_key_down(HWND hwnd, UINT msg, WPARAM wParam, unsigned int keycode, unsigned int modifiers) {
    (void)hwnd;
    (void)msg;
    (void)wParam;
    (void)keycode;
    (void)modifiers;
    return NG_MENU_KEY_PASS;
}

int ng_windows_menu_key_up(HWND hwnd, UINT msg, WPARAM wParam) {
    (void)hwnd;
    (void)msg;
    (void)wParam;
    return NG_MENU_KEY_PASS;
}

void ng_windows_menu_command(unsigned int command_id, LPARAM lParam) {
    (void)command_id;
    (void)lParam;
}
