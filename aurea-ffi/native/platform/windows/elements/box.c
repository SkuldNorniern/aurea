#include "common.h"
#include "../window_state.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include <windows.h>
#include <commctrl.h>
#include <string.h>

#define BOX_SUBCLASS_ID 0xB0

/* Fills the box around its shown children, in the row colour inside a row list. */
static void erase_gaps(HWND hwnd, HDC hdc) {
    int saved = SaveDC(hdc);
    for (HWND child = GetWindow(hwnd, GW_CHILD); child; child = GetWindow(child, GW_HWNDNEXT)) {
        if (!(GetWindowLongPtrA(child, GWL_STYLE) & WS_VISIBLE)) continue;
        RECT r;
        GetWindowRect(child, &r);
        MapWindowPoints(NULL, hwnd, (POINT*)&r, 2);
        ExcludeClipRect(hdc, r.left, r.top, r.right, r.bottom);
    }
    RECT client;
    GetClientRect(hwnd, &client);
    HBRUSH row = ng_windows_row_brush(hwnd);
    FillRect(hdc, &client, row ? row : GetSysColorBrush(COLOR_BTNFACE));
    RestoreDC(hdc, saved);
}

static LRESULT CALLBACK box_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                 UINT_PTR id, DWORD_PTR data) {
    (void)data;
    if (ng_windows_is_notification(msg)) {
        ng_windows_forward_to_window(hwnd, msg, wParam, lParam);
    }
    switch (msg) {
    case WM_ERASEBKGND:
        erase_gaps(hwnd, (HDC)wParam);
        return 1;
    case WM_CTLCOLORSTATIC:
    case WM_CTLCOLORBTN: {
        HBRUSH row = ng_windows_row_brush(hwnd);
        if (!row) break;
        LOGBRUSH fill;
        GetObject(row, sizeof(fill), &fill);
        SetBkColor((HDC)wParam, fill.lbColor);
        return (LRESULT)row;
    }
    case WM_PAINT: {
        /* A box has no text; STATIC would paint over the erase in its parent's colour. */
        PAINTSTRUCT ps;
        BeginPaint(hwnd, &ps);
        EndPaint(hwnd, &ps);
        return 0;
    }
    case WM_SIZE:
        layout_box_children(hwnd);
        break;
    case WM_NCDESTROY:
        ng_windows_remove_subclass(hwnd, box_proc, id);
        break;
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_box(int is_vertical) {
    HWND container = CreateWindowExA(WS_EX_CONTROLPARENT, "STATIC", NULL,
                                     WS_CHILD | SS_LEFT | WS_VISIBLE, 0, 0, 100, 100,
                                     ng_windows_detached_parent(), NULL, GetModuleHandleA(NULL), NULL);
    if (!container) return NULL;
    if (!ng_windows_set_subclass(container, box_proc, BOX_SUBCLASS_ID, 0)) {
        DestroyWindow(container);
        return NULL;
    }
    SetPropA(container, BOX_ORIENTATION_PROP,
             (HANDLE)(INT_PTR)(is_vertical ? BOX_VERTICAL : BOX_HORIZONTAL));
    ng_windows_mark_container(container);
    return (NGHandle)container;
}

int ng_windows_box_add(NGHandle box, NGHandle element) {
    return ng_windows_box_add_weighted(box, element, 0.0f);
}

int ng_windows_box_add_weighted(NGHandle box, NGHandle element, float weight) {
    HWND box_hwnd = (HWND)box;
    HWND element_hwnd = (HWND)element;
    if (!box_hwnd || !element_hwnd || !IsWindow(box_hwnd) || !IsWindow(element_hwnd)) {
        return NG_ERROR_INVALID_HANDLE;
    }

    static INT_PTR next_index = 1;
    SetParent(element_hwnd, box_hwnd);
    SetWindowPos(element_hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    SetPropA(element_hwnd, BOX_INDEX_PROP, (HANDLE)next_index++);
    RECT added;
    GetWindowRect(element_hwnd, &added);
    SetPropA(element_hwnd, BOX_NATURAL_PROP,
             (HANDLE)(((INT_PTR)(added.right - added.left) & 0xFFFF) << 16 |
                      ((INT_PTR)(added.bottom - added.top) & 0xFFFF)));
    if (weight > 0.0f) {
        if (weight > 1e6f) weight = 1e6f;
        SetPropA(element_hwnd, BOX_WEIGHT_PROP, (HANDLE)(INT_PTR)(weight * 1000.0f + 1.0f));
    } else {
        RemovePropA(element_hwnd, BOX_WEIGHT_PROP);
    }

    LONG_PTR style = GetWindowLongPtrA(element_hwnd, GWL_STYLE);
    SetWindowLongPtrA(element_hwnd, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);
    if (!is_box(element_hwnd) && !SendMessageA(element_hwnd, WM_GETFONT, 0, 0)) {
        ng_windows_set_font(element_hwnd, ng_windows_ui_font(box_hwnd), FALSE, TRUE);
    }
    ShowWindow(element_hwnd, SW_SHOW);

    layout_box_children(box_hwnd);
    HWND parent = GetParent(box_hwnd);
    if (parent && ng_windows_window_state(parent)) {
        ng_windows_fit_content(parent);
    } else {
        ng_windows_relayout(box_hwnd);
    }
    return NG_SUCCESS;
}

void ng_windows_box_invalidate(NGHandle box) {
    if (!box) return;
    InvalidateRect((HWND)box, NULL, FALSE);
}
