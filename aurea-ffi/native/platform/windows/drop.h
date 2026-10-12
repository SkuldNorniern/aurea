#ifndef NATIVE_GUI_WINDOWS_DROP_H
#define NATIVE_GUI_WINDOWS_DROP_H

#include <windows.h>

/* Files dragged over `hwnd` report through ng_invoke_file_drop. */
void ng_windows_accept_drops(HWND hwnd);
void ng_windows_refuse_drops(HWND hwnd);

#endif
