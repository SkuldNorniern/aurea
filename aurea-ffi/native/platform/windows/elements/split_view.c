#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>
#include <windowsx.h>
#include <stdlib.h>
#include <string.h>

#define SPLIT_VIEW_OLD_PROC_PROP "AureaSplitViewOldProc"
#define SPLIT_VIEW_MIN_PANE 50
#define SPLIT_VIEW_DIVIDER 6

typedef struct {
    int is_vertical;
    int divider_pos;
    int shown;
    int grab;
    int dragging;
    HWND child1;
    HWND child2;
} SplitViewData;

static int clamp_divider(int pos, int total, int min_pane) {
    if (total <= 0) return 0;
    if (total < min_pane * 2) {
        return total / 2;
    }
    if (pos < min_pane) return min_pane;
    if (pos > total - min_pane) return total - min_pane;
    return pos;
}

static void split_view_layout(HWND hwnd, SplitViewData* data) {
    if (!data) return;

    RECT rect;
    if (!GetClientRect(hwnd, &rect)) return;

    int width = rect.right - rect.left;
    int height = rect.bottom - rect.top;
    if (width <= 0 || height <= 0) return;

    if (!data->child1 && !data->child2) {
        return;
    }

    if (!data->child2) {
        if (data->child1) {
            SetWindowPos(data->child1, NULL, 0, 0, width, height,
                         SWP_NOZORDER | SWP_NOACTIVATE);
        }
        return;
    }

    int bar = ng_windows_px(hwnd, SPLIT_VIEW_DIVIDER);
    int total = data->is_vertical ? height : width;
    int pos = data->divider_pos;
    if (pos <= 0) {
        pos = (total - bar) / 2;
    }
    pos = clamp_divider(pos, total - bar, ng_windows_px(hwnd, SPLIT_VIEW_MIN_PANE));
    data->shown = pos;

    if (data->is_vertical) {
        SetWindowPos(data->child1, NULL, 0, 0, width, pos,
                     SWP_NOZORDER | SWP_NOACTIVATE);
        SetWindowPos(data->child2, NULL, 0, pos + bar, width, height - pos - bar,
                     SWP_NOZORDER | SWP_NOACTIVATE);
    } else {
        SetWindowPos(data->child1, NULL, 0, 0, pos, height,
                     SWP_NOZORDER | SWP_NOACTIVATE);
        SetWindowPos(data->child2, NULL, pos + bar, 0, width - pos - bar, height,
                     SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

static int along(SplitViewData* data, LPARAM lParam) {
    return data->is_vertical ? GET_Y_LPARAM(lParam) : GET_X_LPARAM(lParam);
}

static int on_divider(HWND hwnd, SplitViewData* data, int at) {
    return data->child2 && at >= data->shown &&
           at < data->shown + ng_windows_px(hwnd, SPLIT_VIEW_DIVIDER);
}

static LRESULT CALLBACK SplitViewProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    SplitViewData* data = (SplitViewData*)GetWindowLongPtrA(hwnd, GWLP_USERDATA);
    if (ng_windows_is_notification(msg)) {
        ng_windows_forward_to_window(hwnd, msg, wParam, lParam);
    }

    if (msg == WM_SIZE) {
        split_view_layout(hwnd, data);
    } else if (msg == WM_NCHITTEST) {
        return HTCLIENT;
    } else if (msg == WM_SETCURSOR && data && (HWND)wParam == hwnd) {
        POINT p;
        GetCursorPos(&p);
        ScreenToClient(hwnd, &p);
        if (on_divider(hwnd, data, data->is_vertical ? p.y : p.x)) {
            SetCursor(LoadCursorW(NULL, data->is_vertical ? IDC_SIZENS : IDC_SIZEWE));
            return TRUE;
        }
    } else if (msg == WM_LBUTTONDOWN && data && on_divider(hwnd, data, along(data, lParam))) {
        data->dragging = 1;
        data->grab = along(data, lParam) - data->shown;
        SetCapture(hwnd);
        return 0;
    } else if (msg == WM_MOUSEMOVE && data && data->dragging) {
        data->divider_pos = along(data, lParam) - data->grab;
        if (data->divider_pos <= 0) data->divider_pos = 1;
        split_view_layout(hwnd, data);
        return 0;
    } else if ((msg == WM_LBUTTONUP || msg == WM_CAPTURECHANGED) && data && data->dragging) {
        data->dragging = 0;
        if (msg == WM_LBUTTONUP) ReleaseCapture();
        return 0;
    } else if (msg == WM_ERASEBKGND) {
        RECT client;
        GetClientRect(hwnd, &client);
        FillRect((HDC)wParam, &client, GetSysColorBrush(COLOR_BTNFACE));
        return 1;
    } else if (msg == WM_NCDESTROY) {
        if (data) {
            free(data);
            SetWindowLongPtrA(hwnd, GWLP_USERDATA, 0);
        }
    }

    WNDPROC old_proc = (WNDPROC)GetPropA(hwnd, SPLIT_VIEW_OLD_PROC_PROP);
    if (msg == WM_NCDESTROY) {
        RemovePropA(hwnd, SPLIT_VIEW_OLD_PROC_PROP);
    }
    if (old_proc) {
        return CallWindowProcA(old_proc, hwnd, msg, wParam, lParam);
    }
    return DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_split_view(int is_vertical) {
    HWND temp_parent = ng_windows_detached_parent();

    HWND container = CreateWindowExA(
        WS_EX_CONTROLPARENT,
        "STATIC",
        NULL,
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
        0, 0, 100, 100,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (container) {
        SplitViewData* data = (SplitViewData*)calloc(1, sizeof(SplitViewData));
        if (!data) {
            DestroyWindow(container);
            return NULL;
        }
        data->is_vertical = is_vertical ? 1 : 0;

        SetWindowLongPtrA(container, GWLP_USERDATA, (LONG_PTR)data);
        WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(container, GWLP_WNDPROC, (LONG_PTR)SplitViewProc);
        if (old_proc) {
            SetPropA(container, SPLIT_VIEW_OLD_PROC_PROP, (HANDLE)old_proc);
            ng_windows_mark_container(container);
        }
    }

    return (NGHandle)container;
}

int ng_windows_split_view_add(NGHandle split_handle, NGHandle element) {
    if (!split_handle || !element) return NG_ERROR_INVALID_HANDLE;

    HWND split_hwnd = (HWND)split_handle;
    HWND element_hwnd = (HWND)element;
    SplitViewData* data = (SplitViewData*)GetWindowLongPtrA(split_hwnd, GWLP_USERDATA);
    if (!data) return NG_ERROR_PLATFORM_SPECIFIC;

    if (!data->child1) {
        data->child1 = element_hwnd;
    } else if (!data->child2) {
        data->child2 = element_hwnd;
    } else {
        return NG_ERROR_INVALID_PARAMETER;
    }

    SetParent(element_hwnd, split_hwnd);
    SetWindowPos(element_hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);

    LONG_PTR style = GetWindowLongPtrA(element_hwnd, GWL_STYLE);
    SetWindowLongPtrA(element_hwnd, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);
    if (!is_box(element_hwnd) && !SendMessageW(element_hwnd, WM_GETFONT, 0, 0)) {
        SendMessageW(element_hwnd, WM_SETFONT, (WPARAM)ng_windows_ui_font(split_hwnd), FALSE);
    }

    ShowWindow(element_hwnd, SW_SHOW);
    split_view_layout(split_hwnd, data);

    return NG_SUCCESS;
}

int ng_windows_split_view_set_divider_position(NGHandle split_handle, int index, float position) {
    if (!split_handle) return NG_ERROR_INVALID_HANDLE;
    if (index != 0) return NG_ERROR_INVALID_PARAMETER;
    if (!(position >= 0.0f) || !(position < 1e7f)) return NG_ERROR_INVALID_PARAMETER;

    HWND split_hwnd = (HWND)split_handle;
    SplitViewData* data = (SplitViewData*)GetWindowLongPtrA(split_hwnd, GWLP_USERDATA);
    if (!data) return NG_ERROR_PLATFORM_SPECIFIC;

    int pos = ng_windows_px(split_hwnd, (int)(position + 0.5f));
    data->divider_pos = pos > 0 ? pos : 1;
    split_view_layout(split_hwnd, data);

    return NG_SUCCESS;
}
