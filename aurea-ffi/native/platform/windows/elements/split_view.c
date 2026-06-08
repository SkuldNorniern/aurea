#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>
#include <stdlib.h>
#include <string.h>

#define SPLIT_VIEW_OLD_PROC_PROP "AureaSplitViewOldProc"
#define SPLIT_VIEW_MIN_PANE 50

typedef struct {
    int is_vertical;
    int divider_pos;
    HWND child1;
    HWND child2;
} SplitViewData;

static int clamp_divider(int pos, int total) {
    if (total <= 0) return 0;
    if (total < SPLIT_VIEW_MIN_PANE * 2) {
        return total / 2;
    }
    if (pos < SPLIT_VIEW_MIN_PANE) return SPLIT_VIEW_MIN_PANE;
    if (pos > total - SPLIT_VIEW_MIN_PANE) return total - SPLIT_VIEW_MIN_PANE;
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

    int pos = data->divider_pos;
    if (pos <= 0) {
        pos = data->is_vertical ? height / 2 : width / 2;
    }
    pos = clamp_divider(pos, data->is_vertical ? height : width);
    data->divider_pos = pos;

    if (data->is_vertical) {
        SetWindowPos(data->child1, NULL, 0, 0, width, pos,
                     SWP_NOZORDER | SWP_NOACTIVATE);
        SetWindowPos(data->child2, NULL, 0, pos, width, height - pos,
                     SWP_NOZORDER | SWP_NOACTIVATE);
    } else {
        SetWindowPos(data->child1, NULL, 0, 0, pos, height,
                     SWP_NOZORDER | SWP_NOACTIVATE);
        SetWindowPos(data->child2, NULL, pos, 0, width - pos, height,
                     SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

static LRESULT CALLBACK SplitViewProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    if (msg == WM_COMMAND) {
        HWND parent = GetParent(hwnd);
        while (parent && parent != GetDesktopWindow()) {
            char class_name[256];
            GetClassNameA(parent, class_name, sizeof(class_name));
            if (_stricmp(class_name, "NativeGuiWindow") == 0) {
                SendMessageA(parent, msg, wParam, lParam);
                break;
            }
            parent = GetParent(parent);
        }
    }

    if (msg == WM_SIZE) {
        SplitViewData* data = (SplitViewData*)GetWindowLongPtrA(hwnd, GWLP_USERDATA);
        split_view_layout(hwnd, data);
    } else if (msg == WM_NCDESTROY) {
        SplitViewData* data = (SplitViewData*)GetWindowLongPtrA(hwnd, GWLP_USERDATA);
        if (data) {
            free(data);
            SetWindowLongPtrA(hwnd, GWLP_USERDATA, 0);
        }
        RemovePropA(hwnd, SPLIT_VIEW_OLD_PROC_PROP);
    }

    WNDPROC old_proc = (WNDPROC)GetPropA(hwnd, SPLIT_VIEW_OLD_PROC_PROP);
    if (old_proc) {
        return CallWindowProcA(old_proc, hwnd, msg, wParam, lParam);
    }
    return DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_split_view(int is_vertical) {
    HWND temp_parent = GetDesktopWindow();

    HWND container = CreateWindowExA(
        0,
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
        data->divider_pos = 0;
        data->child1 = NULL;
        data->child2 = NULL;

        SetWindowLongPtrA(container, GWLP_USERDATA, (LONG_PTR)data);
        SetClassLongPtrA(container, GCLP_HBRBACKGROUND, (LONG_PTR)GetStockObject(NULL_BRUSH));
        WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(container, GWLP_WNDPROC, (LONG_PTR)SplitViewProc);
        if (old_proc) {
            SetPropA(container, SPLIT_VIEW_OLD_PROC_PROP, (HANDLE)old_proc);
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

    LONG_PTR style = GetWindowLongPtrA(element_hwnd, GWL_STYLE);
    SetWindowLongPtrA(element_hwnd, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);

    ShowWindow(element_hwnd, SW_SHOW);
    split_view_layout(split_hwnd, data);

    return NG_SUCCESS;
}

int ng_windows_split_view_set_divider_position(NGHandle split_handle, int index, float position) {
    if (!split_handle) return NG_ERROR_INVALID_HANDLE;
    if (index != 0) return NG_ERROR_INVALID_PARAMETER;

    HWND split_hwnd = (HWND)split_handle;
    SplitViewData* data = (SplitViewData*)GetWindowLongPtrA(split_hwnd, GWLP_USERDATA);
    if (!data) return NG_ERROR_PLATFORM_SPECIFIC;

    data->divider_pos = (int)position;
    split_view_layout(split_hwnd, data);

    return NG_SUCCESS;
}
