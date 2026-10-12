#ifndef AUREA_WINDOWS_PEN_H
#define AUREA_WINDOWS_PEN_H

#include <windows.h>

/* Reports a pen's WM_POINTERDOWN, WM_POINTERUPDATE or WM_POINTERUP to root's
 * window as pen samples, positions in root's client area. Other pointers and
 * messages are left alone. The message still goes to DefWindowProc after,
 * which makes the mouse messages for the same pen. */
void ng_windows_pen_message(HWND root, UINT msg, WPARAM wParam);

#endif
