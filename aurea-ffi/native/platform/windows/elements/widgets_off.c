#include "common.h"

/* Without the widgets the layout is asked about no control, so every answer is no. */
int ng_windows_is_rich_edit(HWND hwnd) {
    (void)hwnd;
    return 0;
}

int ng_windows_is_scroll_view(HWND hwnd) {
    (void)hwnd;
    return 0;
}

int ng_windows_is_split_view(HWND hwnd) {
    (void)hwnd;
    return 0;
}

int ng_windows_tab_bar_measure(HWND bar, int* width, int* height) {
    (void)bar;
    (void)width;
    (void)height;
    return 0;
}

void ng_windows_scroll_view_layout(HWND scroll) {
    (void)scroll;
}

int ng_windows_button_command(unsigned int command_id) {
    (void)command_id;
    return 0;
}
