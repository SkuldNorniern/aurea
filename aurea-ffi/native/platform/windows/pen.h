#ifndef AUREA_WINDOWS_PEN_H
#define AUREA_WINDOWS_PEN_H

#include <windows.h>

#define AUREA_PEN_HOVER 0
#define AUREA_PEN_DOWN 1
#define AUREA_PEN_MOVE 2
#define AUREA_PEN_UP 3
#define AUREA_PEN_CANCEL 5

#define AUREA_PEN_ERASER 1u
#define AUREA_PEN_BARREL 2u
#define AUREA_PEN_WINTAB 4u

#define AUREA_PEN_MODE_AUTO 0
#define AUREA_PEN_MODE_SYSTEM 1
#define AUREA_PEN_MODE_WINTAB 2

/* Reports a pen's WM_POINTERDOWN, WM_POINTERUPDATE or WM_POINTERUP to root's window as pen samples, positions in root's client area. */
void ng_windows_pen_message(HWND root, UINT msg, WPARAM wParam);
/* WM_POINTERCAPTURECHANGED: the system took the pen, so a stroke it was drawing is dropped. */
void ng_windows_pen_capture_lost(HWND root, WPARAM wParam);
int ng_windows_pen_mode(void);
void ng_windows_pen_set_mode(int mode);
/* Whether Windows Ink is sending the pen, so wintab stays quiet. */
int ng_windows_pen_ink_live(void);

/* Keeps the wintab context with the active window. */
void ng_windows_wintab_activate(HWND root, int active);
void ng_windows_wintab_destroyed(HWND root);
void ng_windows_wintab_sync(void);
/* Drops a wintab stroke in progress, when Windows Ink starts sending the pen. */
void ng_windows_wintab_yield(void);
/* WT_PACKET and WT_PROXIMITY; any other message is ignored. */
void ng_windows_wintab_message(HWND root, UINT msg, LPARAM lParam);

#endif
