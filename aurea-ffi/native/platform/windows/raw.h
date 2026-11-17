#ifndef AUREA_WINDOWS_RAW_H
#define AUREA_WINDOWS_RAW_H

#include <windows.h>

/* Registers raw mouse input for `hwnd` when `raw` is set; drops it when `hwnd` held it and `raw` is not. */
void ng_windows_raw_mouse_take(HWND hwnd, int raw);
/* WM_INPUT for `hwnd`: relative motion for a window in a raw grab. */
void ng_windows_raw_input(HWND hwnd, LPARAM lParam);

#endif
