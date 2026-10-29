#ifndef AUREA_WINDOWS_GESTURE_H
#define AUREA_WINDOWS_GESTURE_H

#include <windows.h>

/* Asks for the zoom and rotate gestures of a touchscreen to come as WM_GESTURE to hwnd. */
void ng_windows_enable_gestures(HWND hwnd);

/* Reports a WM_GESTURE's zoom or rotate to hwnd's window; 1 when it was one of those and is done with. */
int ng_windows_gesture_message(HWND hwnd, LPARAM lParam);

#endif
