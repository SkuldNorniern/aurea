#include "common.h"
#include "../elements.h"
#include "../../common/errors.h"
#include <windows.h>

NGHandle ng_windows_create_checkbox(const char* label) {
    HWND hwnd = CreateWindowExA(
        0,
        "BUTTON",
        label ? label : "",
        WS_CHILD | WS_VISIBLE | BS_AUTOCHECKBOX,
        0, 0, 200, 25,
        GetDesktopWindow(),
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    return (NGHandle)hwnd;
}

int ng_windows_checkbox_set_checked(NGHandle checkbox, int checked) {
    if (!checkbox) return NG_ERROR_INVALID_HANDLE;
    
    HWND hwnd = (HWND)checkbox;
    SendMessage(hwnd, BM_SETCHECK, checked ? BST_CHECKED : BST_UNCHECKED, 0);
    return NG_SUCCESS;
}

int ng_windows_checkbox_get_checked(NGHandle checkbox) {
    if (!checkbox) return 0;
    
    HWND hwnd = (HWND)checkbox;
    return SendMessage(hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED ? 1 : 0;
}

int ng_windows_checkbox_set_enabled(NGHandle checkbox, int enabled) {
    if (!checkbox) return NG_ERROR_INVALID_HANDLE;
    
    EnableWindow((HWND)checkbox, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_windows_checkbox_invalidate(NGHandle checkbox) {
    if (!checkbox) return;
    InvalidateRect((HWND)checkbox, NULL, FALSE);
}

