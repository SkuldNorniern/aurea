#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <stdlib.h>

#define SIDEBAR_OLD_PROC_PROP "AureaSidebarOldProc"
#define SIDEBAR_DATA_PROP "AureaSidebarData"
#define SIDEBAR_BASE_ID 9500
#define ROW_HEIGHT 24
#define SECTION_PADDING 4
#define INDENT_STEP 10
#define LEFT_MARGIN 6

typedef struct {
    HWND hwnd;
    int indent;
    int item;
} SidebarRow;

typedef struct {
    unsigned int id;
    SidebarRow* rows;
    int row_count;
    int row_cap;
    int item_count;
    int selected;
    int scroll;
} SidebarData;

static SidebarData* sidebar_data(HWND bar) {
    return (SidebarData*)GetPropA(bar, SIDEBAR_DATA_PROP);
}

static void sidebar_show_selected(SidebarData* data) {
    for (int i = 0; i < data->row_count; i++) {
        SidebarRow* row = &data->rows[i];
        if (row->item < 0) continue;
        SendMessageW(row->hwnd, BM_SETCHECK,
                     row->item == data->selected ? BST_CHECKED : BST_UNCHECKED, 0);
    }
}

static int sidebar_row_height(HWND bar, const SidebarRow* row) {
    int h = ng_windows_px(bar, ROW_HEIGHT);
    return row->item < 0 ? h + ng_windows_px(bar, SECTION_PADDING) : h;
}

static void sidebar_layout(HWND bar) {
    SidebarData* data = sidebar_data(bar);
    if (!data) return;
    RECT client;
    GetClientRect(bar, &client);
    int total = 0;
    for (int i = 0; i < data->row_count; i++) total += sidebar_row_height(bar, &data->rows[i]);

    int room = client.bottom > 0 ? client.bottom : 0;
    int max_scroll = total > room ? total - room : 0;
    if (data->scroll > max_scroll) data->scroll = max_scroll;
    if (data->scroll < 0) data->scroll = 0;
    SCROLLINFO info = {sizeof(info), SIF_RANGE | SIF_PAGE | SIF_POS, 0, total > 0 ? total - 1 : 0,
                       (UINT)room, data->scroll, 0};
    SetScrollInfo(bar, SB_VERT, &info, TRUE);

    int y = -data->scroll;
    for (int i = 0; i < data->row_count; i++) {
        SidebarRow* row = &data->rows[i];
        int x = ng_windows_px(bar, LEFT_MARGIN + row->indent * INDENT_STEP);
        int h = ng_windows_px(bar, ROW_HEIGHT) - (row->item < 0 ? 0 : ng_windows_px(bar, 2));
        int w = client.right - x;
        SetWindowPos(row->hwnd, NULL, x, y, w > 0 ? w : 0, h, SWP_NOZORDER | SWP_NOACTIVATE);
        y += sidebar_row_height(bar, row);
    }
}

static void sidebar_scroll_to(HWND bar, SidebarData* data, int scroll) {
    data->scroll = scroll;
    sidebar_layout(bar);
    InvalidateRect(bar, NULL, TRUE);
}

static LRESULT CALLBACK SidebarProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    SidebarData* data = sidebar_data(hwnd);
    if (msg == WM_COMMAND && data && HIWORD(wParam) == BN_CLICKED) {
        int idx = (int)LOWORD(wParam) - SIDEBAR_BASE_ID;
        if (idx >= 0 && idx < data->item_count) {
            int changed = idx != data->selected;
            data->selected = idx;
            sidebar_show_selected(data);
            if (changed) ng_invoke_sidebar_list_selected(data->id, idx);
            return 0;
        }
    } else if (msg == WM_SIZE) {
        sidebar_layout(hwnd);
    } else if (msg == WM_ERASEBKGND) {
        RECT client;
        GetClientRect(hwnd, &client);
        FillRect((HDC)wParam, &client, GetSysColorBrush(COLOR_BTNFACE));
        return 1;
    } else if (msg == WM_VSCROLL && data && !lParam) {
        int line = ng_windows_px(hwnd, ROW_HEIGHT);
        RECT client;
        GetClientRect(hwnd, &client);
        int at = data->scroll;
        switch (LOWORD(wParam)) {
            case SB_LINEUP: at -= line; break;
            case SB_LINEDOWN: at += line; break;
            case SB_PAGEUP: at -= client.bottom; break;
            case SB_PAGEDOWN: at += client.bottom; break;
            case SB_THUMBTRACK:
            case SB_THUMBPOSITION: {
                SCROLLINFO info = {sizeof(info), SIF_TRACKPOS};
                GetScrollInfo(hwnd, SB_VERT, &info);
                at = info.nTrackPos;
                break;
            }
            default: break;
        }
        sidebar_scroll_to(hwnd, data, at);
        return 0;
    } else if (msg == WM_MOUSEWHEEL && data) {
        int notches = GET_WHEEL_DELTA_WPARAM(wParam) / WHEEL_DELTA;
        sidebar_scroll_to(hwnd, data, data->scroll - notches * 3 * ng_windows_px(hwnd, ROW_HEIGHT));
        return 0;
    } else if (msg == WM_NCDESTROY) {
        if (data) free(data->rows);
        free(data);
        RemovePropA(hwnd, SIDEBAR_DATA_PROP);
    }

    WNDPROC old_proc = (WNDPROC)GetPropA(hwnd, SIDEBAR_OLD_PROC_PROP);
    if (msg == WM_NCDESTROY) RemovePropA(hwnd, SIDEBAR_OLD_PROC_PROP);
    if (old_proc) {
        return CallWindowProcA(old_proc, hwnd, msg, wParam, lParam);
    }
    return DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_sidebar_list(unsigned int id) {
    HWND container = CreateWindowExA(
        0,
        "STATIC",
        NULL,
        WS_CHILD | SS_LEFT | WS_VISIBLE | WS_VSCROLL | WS_CLIPCHILDREN,
        0, 0, 200, 400,
        ng_windows_detached_parent(),
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    if (!container) return NULL;

    SidebarData* data = (SidebarData*)calloc(1, sizeof(SidebarData));
    if (!data) {
        DestroyWindow(container);
        return NULL;
    }
    data->id = id;
    data->selected = -1;
    SetPropA(container, SIDEBAR_DATA_PROP, (HANDLE)data);
    WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(container, GWLP_WNDPROC, (LONG_PTR)SidebarProc);
    if (old_proc) {
        SetPropA(container, SIDEBAR_OLD_PROC_PROP, (HANDLE)old_proc);
    }
    return (NGHandle)container;
}

static int sidebar_add_row(HWND bar, SidebarData* data, HWND row, int indent, int item) {
    if (data->row_count == data->row_cap) {
        int cap = data->row_cap ? data->row_cap * 2 : 16;
        SidebarRow* grown = (SidebarRow*)realloc(data->rows, (size_t)cap * sizeof(SidebarRow));
        if (!grown) {
            DestroyWindow(row);
            return NG_ERROR_CREATION_FAILED;
        }
        data->rows = grown;
        data->row_cap = cap;
    }
    data->rows[data->row_count++] = (SidebarRow){row, indent > 0 ? indent : 0, item};
    SendMessageW(row, WM_SETFONT, (WPARAM)ng_windows_ui_font(bar), FALSE);
    sidebar_layout(bar);
    return NG_SUCCESS;
}

int ng_windows_sidebar_list_add_section(NGHandle sidebar, const char* title) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    HWND bar = (HWND)sidebar;
    SidebarData* data = sidebar_data(bar);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    HWND label = ng_windows_create_control(
        0, L"STATIC", title, WS_CHILD | SS_LEFT | SS_CENTERIMAGE | WS_VISIBLE, 200, ROW_HEIGHT, bar, 0);
    if (!label) return NG_ERROR_CREATION_FAILED;
    return sidebar_add_row(bar, data, label, 0, -1);
}

int ng_windows_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    HWND bar = (HWND)sidebar;
    SidebarData* data = sidebar_data(bar);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    HWND btn = ng_windows_create_control(
        0, L"BUTTON", title, WS_CHILD | WS_VISIBLE | BS_CHECKBOX | BS_PUSHLIKE | BS_LEFT,
        200, ROW_HEIGHT, bar, SIDEBAR_BASE_ID + data->item_count);
    if (!btn) return NG_ERROR_CREATION_FAILED;
    int result = sidebar_add_row(bar, data, btn, indent, data->item_count);
    if (result == NG_SUCCESS) data->item_count++;
    return result;
}

int ng_windows_sidebar_list_set_selected(NGHandle sidebar, int index) {
    SidebarData* data = sidebar ? sidebar_data((HWND)sidebar) : NULL;
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= data->item_count) return NG_ERROR_INVALID_PARAMETER;
    data->selected = index;
    sidebar_show_selected(data);
    return NG_SUCCESS;
}

int ng_windows_sidebar_list_get_selected(NGHandle sidebar) {
    SidebarData* data = sidebar ? sidebar_data((HWND)sidebar) : NULL;
    return data ? data->selected : -1;
}

int ng_windows_sidebar_list_clear(NGHandle sidebar) {
    SidebarData* data = sidebar ? sidebar_data((HWND)sidebar) : NULL;
    if (!data) return NG_ERROR_INVALID_HANDLE;
    for (int i = 0; i < data->row_count; i++) DestroyWindow(data->rows[i].hwnd);
    data->row_count = 0;
    data->item_count = 0;
    data->selected = -1;
    data->scroll = 0;
    sidebar_layout((HWND)sidebar);
    return NG_SUCCESS;
}

void ng_windows_sidebar_list_invalidate(NGHandle sidebar) {
    if (!sidebar) return;
    InvalidateRect((HWND)sidebar, NULL, FALSE);
}
