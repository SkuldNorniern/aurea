#ifndef AUREA_WIN32_H
#define AUREA_WIN32_H

#include <windows.h>
#include <commctrl.h>
#ifdef __cplusplus
extern "C" {
#endif
/* For native Win32 code next to Aurea. Importing comctl32 loads v5 before Aurea switches to v6, so
   subclass through these, which find v6 at run time. */
BOOL WINAPI ng_windows_set_subclass(HWND hwnd, SUBCLASSPROC proc, UINT_PTR id, DWORD_PTR data);
BOOL WINAPI ng_windows_get_subclass(HWND hwnd, SUBCLASSPROC proc, UINT_PTR id, DWORD_PTR* data);
BOOL WINAPI ng_windows_remove_subclass(HWND hwnd, SUBCLASSPROC proc, UINT_PTR id);
LRESULT WINAPI ng_windows_def_subclass(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
/* The hidden parent to create a native child under before it is added to a container. */
HWND ng_windows_detached_parent(void);
#ifdef __cplusplus
}
#endif

#endif
