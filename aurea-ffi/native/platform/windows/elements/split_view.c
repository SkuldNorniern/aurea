#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include <windows.h>
#include <windowsx.h>
#include <stdlib.h>

#define SPLIT_VIEW_SUBCLASS_ID 1
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
} SplitViewState;

static LRESULT CALLBACK split_view_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                        UINT_PTR id, DWORD_PTR data);

static SplitViewState* split_state(HWND hwnd) {
    DWORD_PTR data = 0;
    return ng_windows_get_subclass(hwnd, split_view_proc, SPLIT_VIEW_SUBCLASS_ID, &data)
               ? (SplitViewState*)data
               : NULL;
}

static int clamp_divider(int pos, int total, int min_pane) {
    if (total <= 0) return 0;
    if (total < min_pane * 2) return total / 2;
    if (pos < min_pane) return min_pane;
    if (pos > total - min_pane) return total - min_pane;
    return pos;
}

static void split_view_layout(HWND hwnd, SplitViewState* s) {
    RECT rect;
    if (!GetClientRect(hwnd, &rect)) return;
    int width = rect.right - rect.left;
    int height = rect.bottom - rect.top;
    if (width <= 0 || height <= 0) return;

    if (!s->child2) {
        if (s->child1) {
            SetWindowPos(s->child1, NULL, 0, 0, width, height, SWP_NOZORDER | SWP_NOACTIVATE);
        }
        return;
    }

    int bar = ng_windows_px(hwnd, SPLIT_VIEW_DIVIDER);
    int total = s->is_vertical ? height : width;
    int pos = s->divider_pos;
    if (pos <= 0) pos = (total - bar) / 2;
    pos = clamp_divider(pos, total - bar, ng_windows_px(hwnd, SPLIT_VIEW_MIN_PANE));
    s->shown = pos;

    if (s->is_vertical) {
        SetWindowPos(s->child1, NULL, 0, 0, width, pos, SWP_NOZORDER | SWP_NOACTIVATE);
        SetWindowPos(s->child2, NULL, 0, pos + bar, width, height - pos - bar,
                     SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS);
    } else {
        SetWindowPos(s->child1, NULL, 0, 0, pos, height, SWP_NOZORDER | SWP_NOACTIVATE);
        SetWindowPos(s->child2, NULL, pos + bar, 0, width - pos - bar, height,
                     SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS);
    }
}

static int along(const SplitViewState* s, LPARAM lParam) {
    return s->is_vertical ? GET_Y_LPARAM(lParam) : GET_X_LPARAM(lParam);
}

static int on_divider(HWND hwnd, const SplitViewState* s, int at) {
    return s->child2 && at >= s->shown && at < s->shown + ng_windows_px(hwnd, SPLIT_VIEW_DIVIDER);
}

static LRESULT CALLBACK split_view_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                        UINT_PTR id, DWORD_PTR data) {
    SplitViewState* s = (SplitViewState*)data;
    if (ng_windows_is_notification(msg)) {
        ng_windows_forward_to_window(hwnd, msg, wParam, lParam);
    }

    switch (msg) {
    case WM_SIZE:
        split_view_layout(hwnd, s);
        break;
    case WM_NCHITTEST:
        return HTCLIENT;
    case WM_SETCURSOR:
        if ((HWND)wParam == hwnd) {
            POINT p;
            GetCursorPos(&p);
            ScreenToClient(hwnd, &p);
            if (on_divider(hwnd, s, s->is_vertical ? p.y : p.x)) {
                SetCursor(LoadCursor(NULL, s->is_vertical ? IDC_SIZENS : IDC_SIZEWE));
                return TRUE;
            }
        }
        break;
    case WM_LBUTTONDOWN:
        if (on_divider(hwnd, s, along(s, lParam))) {
            s->dragging = 1;
            s->grab = along(s, lParam) - s->shown;
            SetCapture(hwnd);
            return 0;
        }
        break;
    case WM_MOUSEMOVE:
        if (s->dragging) {
            s->divider_pos = along(s, lParam) - s->grab;
            if (s->divider_pos <= 0) s->divider_pos = 1;
            split_view_layout(hwnd, s);
            return 0;
        }
        break;
    case WM_LBUTTONUP:
    case WM_CAPTURECHANGED:
        if (s->dragging) {
            s->dragging = 0;
            if (msg == WM_LBUTTONUP) ReleaseCapture();
            return 0;
        }
        break;
    case WM_ERASEBKGND: {
        RECT client;
        GetClientRect(hwnd, &client);
        FillRect((HDC)wParam, &client, GetSysColorBrush(COLOR_BTNFACE));
        return 1;
    }
    case WM_NCDESTROY:
        ng_windows_remove_subclass(hwnd, split_view_proc, id);
        free(s);
        break;
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_split_view(int is_vertical) {
    HWND container = CreateWindowExA(
        WS_EX_CONTROLPARENT,
        "STATIC",
        NULL,
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
        0, 0, 100, 100,
        ng_windows_detached_parent(),
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    if (!container) return NULL;

    SplitViewState* s = (SplitViewState*)calloc(1, sizeof(SplitViewState));
    if (!s) {
        DestroyWindow(container);
        return NULL;
    }
    s->is_vertical = is_vertical ? 1 : 0;
    if (!ng_windows_set_subclass(container, split_view_proc, SPLIT_VIEW_SUBCLASS_ID, (DWORD_PTR)s)) {
        free(s);
        DestroyWindow(container);
        return NULL;
    }
    ng_windows_mark_container(container);
    return (NGHandle)container;
}

int ng_windows_is_split_view(HWND hwnd) {
    return hwnd && split_state(hwnd) != NULL;
}

int ng_windows_split_view_add(NGHandle split_handle, NGHandle element) {
    if (!split_handle || !element) return NG_ERROR_INVALID_HANDLE;

    HWND split_hwnd = (HWND)split_handle;
    HWND element_hwnd = (HWND)element;
    if (!IsWindow(split_hwnd) || !IsWindow(element_hwnd)) return NG_ERROR_INVALID_HANDLE;

    SplitViewState* s = split_state(split_hwnd);
    if (!s) return NG_ERROR_PLATFORM_SPECIFIC;
    if (element_hwnd == s->child1 || element_hwnd == s->child2) return NG_ERROR_INVALID_PARAMETER;

    if (!s->child1) {
        s->child1 = element_hwnd;
    } else if (!s->child2) {
        s->child2 = element_hwnd;
    } else {
        return NG_ERROR_INVALID_PARAMETER;
    }

    SetParent(element_hwnd, split_hwnd);
    SetWindowPos(element_hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);

    LONG_PTR style = GetWindowLongPtrA(element_hwnd, GWL_STYLE);
    SetWindowLongPtrA(element_hwnd, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);
    if (!is_box(element_hwnd) && !SendMessageW(element_hwnd, WM_GETFONT, 0, 0)) {
        ng_windows_set_font(element_hwnd, ng_windows_ui_font(split_hwnd), FALSE, TRUE);
    }

    ShowWindow(element_hwnd, SW_SHOW);
    split_view_layout(split_hwnd, s);
    return NG_SUCCESS;
}

int ng_windows_split_view_set_divider_position(NGHandle split_handle, int index, float position) {
    if (!split_handle) return NG_ERROR_INVALID_HANDLE;
    if (index != 0) return NG_ERROR_INVALID_PARAMETER;
    if (!(position >= 0.0f) || !(position < 1e7f)) return NG_ERROR_INVALID_PARAMETER;

    HWND split_hwnd = (HWND)split_handle;
    if (!IsWindow(split_hwnd)) return NG_ERROR_INVALID_HANDLE;
    SplitViewState* s = split_state(split_hwnd);
    if (!s) return NG_ERROR_PLATFORM_SPECIFIC;

    int pos = ng_windows_px(split_hwnd, (int)(position + 0.5f));
    s->divider_pos = pos > 0 ? pos : 1;
    split_view_layout(split_hwnd, s);
    return NG_SUCCESS;
}
