#include "common.h"
#include "../elements.h"
#include "../../common/errors.h"
#include <windows.h>
#include <commctrl.h>

#pragma comment(lib, "comctl32.lib")

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
        GetDesktopWindow(),
        NULL,
        GetModuleHandle(NULL),
        NULL
    );
    
    if (!hwnd) return NULL;
    
    SendMessage(hwnd, PBM_SETRANGE, 0, MAKELPARAM(0, 100));
    SendMessage(hwnd, PBM_SETPOS, 0, 0);
    
    return (NGHandle)hwnd;
}

int ng_windows_progress_bar_set_value(NGHandle progress_bar, double value) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    if (value < 0.0) value = 0.0;
    if (value > 1.0) value = 1.0;
    
    HWND hwnd = (HWND)progress_bar;
    int pos = (int)(value * 100.0);
    SendMessage(hwnd, PBM_SETPOS, pos, 0);
    InvalidateRect(hwnd, NULL, FALSE);
    
    return NG_SUCCESS;
}

int ng_windows_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    HWND hwnd = (HWND)progress_bar;
    
    if (indeterminate) {
        SendMessage(hwnd, PBM_SETMARQUEE, TRUE, 0);
    } else {
        SendMessage(hwnd, PBM_SETMARQUEE, FALSE, 0);
    }
    
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

