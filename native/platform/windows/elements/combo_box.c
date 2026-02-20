#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>

NGHandle ng_windows_create_combo_box(void) {
    HWND hwnd = CreateWindowExA(
        WS_EX_CLIENTEDGE,
        "COMBOBOX",
        NULL,
        WS_CHILD | WS_VISIBLE | CBS_DROPDOWNLIST | CBS_HASSTRINGS,
        0, 0, 200, 200,
        GetDesktopWindow(),
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    return (NGHandle)hwnd;
}

int ng_windows_combo_box_add_item(NGHandle combo_box, const char* item) {
    if (!combo_box || !item) return NG_ERROR_INVALID_PARAMETER;
    
    HWND hwnd = (HWND)combo_box;
    int index = (int)SendMessageA(hwnd, CB_ADDSTRING, 0, (LPARAM)item);
    
    if (index == CB_ERR || index == CB_ERRSPACE) {
        return NG_ERROR_CREATION_FAILED;
    }
    
    return NG_SUCCESS;
}

int ng_windows_combo_box_set_selected(NGHandle combo_box, int index) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    HWND hwnd = (HWND)combo_box;
    int count = (int)SendMessage(hwnd, CB_GETCOUNT, 0, 0);
    
    if (index < 0 || index >= count) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    
    SendMessage(hwnd, CB_SETCURSEL, index, 0);
    return NG_SUCCESS;
}

int ng_windows_combo_box_get_selected(NGHandle combo_box) {
    if (!combo_box) return -1;
    
    HWND hwnd = (HWND)combo_box;
    return (int)SendMessage(hwnd, CB_GETCURSEL, 0, 0);
}

int ng_windows_combo_box_clear(NGHandle combo_box) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    HWND hwnd = (HWND)combo_box;
    SendMessage(hwnd, CB_RESETCONTENT, 0, 0);
    return NG_SUCCESS;
}

int ng_windows_combo_box_set_enabled(NGHandle combo_box, int enabled) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    EnableWindow((HWND)combo_box, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_windows_combo_box_invalidate(NGHandle combo_box) {
    if (!combo_box) return;
    InvalidateRect((HWND)combo_box, NULL, FALSE);
}

