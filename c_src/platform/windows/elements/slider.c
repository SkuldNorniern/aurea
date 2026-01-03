#include "common.h"
#include "../elements.h"
#include "../../../common/errors.h"
#include <windows.h>
#include <commctrl.h>

#pragma comment(lib, "comctl32.lib")

NGHandle ng_windows_create_slider(double min, double max) {
    if (min >= max) return NULL;
    
    HWND hwnd = CreateWindowEx(
        0,
        TRACKBAR_CLASS,
        NULL,
        WS_CHILD | WS_VISIBLE | TBS_HORZ | TBS_AUTOTICKS,
        0, 0, 200, 30,
        GetDesktopWindow(),
        NULL,
        GetModuleHandle(NULL),
        NULL
    );
    
    if (!hwnd) return NULL;
    
    SendMessage(hwnd, TBM_SETRANGEMIN, FALSE, (LPARAM)(int)min);
    SendMessage(hwnd, TBM_SETRANGEMAX, TRUE, (LPARAM)(int)max);
    SendMessage(hwnd, TBM_SETPOS, TRUE, (LPARAM)(int)((min + max) / 2.0));
    
    return (NGHandle)hwnd;
}

int ng_windows_slider_set_value(NGHandle slider, double value) {
    if (!slider) return NG_ERROR_INVALID_HANDLE;
    
    HWND hwnd = (HWND)slider;
    int minVal = (int)SendMessage(hwnd, TBM_GETRANGEMIN, 0, 0);
    int maxVal = (int)SendMessage(hwnd, TBM_GETRANGEMAX, 0, 0);
    int intValue = (int)value;
    
    if (intValue < minVal) intValue = minVal;
    if (intValue > maxVal) intValue = maxVal;
    
    SendMessage(hwnd, TBM_SETPOS, TRUE, (LPARAM)intValue);
    return NG_SUCCESS;
}

double ng_windows_slider_get_value(NGHandle slider) {
    if (!slider) return 0.0;
    
    HWND hwnd = (HWND)slider;
    return (double)SendMessage(hwnd, TBM_GETPOS, 0, 0);
}

int ng_windows_slider_set_enabled(NGHandle slider, int enabled) {
    if (!slider) return NG_ERROR_INVALID_HANDLE;
    
    EnableWindow((HWND)slider, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_windows_slider_invalidate(NGHandle slider) {
    if (!slider) return;
    InvalidateRect((HWND)slider, NULL, FALSE);
}

