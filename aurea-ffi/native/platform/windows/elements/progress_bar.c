#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>
#include <commctrl.h>

#pragma comment(lib, "comctl32.lib")

#define PROGRESS_STEPS 10000

NGHandle ng_windows_create_progress_bar(void) {
    INITCOMMONCONTROLSEX icex;
    icex.dwSize = sizeof(INITCOMMONCONTROLSEX);
    icex.dwICC = ICC_PROGRESS_CLASS;
    InitCommonControlsEx(&icex);
    
    HWND hwnd = CreateWindowEx(
        0,
        PROGRESS_CLASS,
        NULL,
        WS_CHILD | WS_VISIBLE | PBS_SMOOTH,
        0, 0, 200, 20,
        ng_windows_detached_parent(),
        NULL,
        GetModuleHandle(NULL),
        NULL
    );
    
    if (!hwnd) return NULL;
    
    SendMessage(hwnd, PBM_SETRANGE32, 0, PROGRESS_STEPS);
    SendMessage(hwnd, PBM_SETPOS, 0, 0);
    
    return (NGHandle)hwnd;
}

int ng_windows_progress_bar_set_value(NGHandle progress_bar, double value) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    if (!(value >= 0.0)) value = 0.0;
    if (value > 1.0) value = 1.0;
    
    HWND hwnd = (HWND)progress_bar;
    if (GetWindowLongPtrW(hwnd, GWL_STYLE) & PBS_MARQUEE) ng_windows_progress_bar_set_indeterminate(progress_bar, 0);
    int pos = (int)(value * PROGRESS_STEPS + 0.5);
    SendMessage(hwnd, PBM_SETPOS, pos, 0);
    InvalidateRect(hwnd, NULL, FALSE);
    
    return NG_SUCCESS;
}

int ng_windows_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    HWND hwnd = (HWND)progress_bar;
    LONG_PTR style = GetWindowLongPtrW(hwnd, GWL_STYLE);
    style = indeterminate ? (style | PBS_MARQUEE) : (style & ~(LONG_PTR)PBS_MARQUEE);
    SetWindowLongPtrW(hwnd, GWL_STYLE, style);
    SendMessageW(hwnd, PBM_SETMARQUEE, indeterminate ? TRUE : FALSE, 0);
    InvalidateRect(hwnd, NULL, TRUE);
    return NG_SUCCESS;
}

int ng_windows_progress_bar_set_enabled(NGHandle progress_bar, int enabled) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    EnableWindow((HWND)progress_bar, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_windows_progress_bar_invalidate(NGHandle progress_bar) {
    if (!progress_bar) return;
    InvalidateRect((HWND)progress_bar, NULL, FALSE);
}

