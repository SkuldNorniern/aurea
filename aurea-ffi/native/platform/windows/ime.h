#ifndef AUREA_WINDOWS_IME_H
#define AUREA_WINDOWS_IME_H

#include "common/types.h"
#include <windows.h>

int ng_windows_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height);

/* Handles WM_IME_ for a window. */
int ng_windows_ime_message(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam, LRESULT* result);

/* Drops the window's stored caret area. */
void ng_windows_ime_release(HWND hwnd);

#endif
