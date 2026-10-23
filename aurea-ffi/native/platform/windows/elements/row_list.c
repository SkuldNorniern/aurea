#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <oleacc.h>
#include <stdlib.h>
#include <wchar.h>

/* A scroll view of row boxes. Labels and boxes are click-through STATIC windows, so plain
   clicks land on the scroll view, and clicks on controls reach it as WM_PARENTNOTIFY. */

#define ROW_LIST_DATA_PROP "AureaRowListData"
#define ROW_LIST_SUBCLASS_ID 0x70
#define ROW_SUBCLASS_ID 0x71
#define ROW_PADDING 6

typedef struct {
    unsigned int id;
    HWND scroll;
    HWND rows;
    HWND* items;
    int count;
    int capacity;
    int selected;
    HWND last_click;
    DWORD last_click_time;
} RowListData;

static RowListData* list_data(HWND scroll) {
    return (RowListData*)GetPropA(scroll, ROW_LIST_DATA_PROP);
}

static RowListData* data_of_row(HWND row) {
    HWND rows = GetParent(row);
    return rows ? list_data(GetParent(rows)) : NULL;
}

static int index_of(RowListData* data, HWND row) {
    for (int i = 0; i < data->count; i++) {
        if (data->items[i] == row) return i;
    }
    return -1;
}

static void relayout(RowListData* data) {
    for (int i = 0; i < data->count; i++) {
        SetPropA(data->items[i], BOX_INDEX_PROP, (HANDLE)(INT_PTR)(i + 1));
    }
    ng_windows_scroll_view_layout(data->scroll);
    layout_box_children(data->rows);
}

static void repaint(HWND row) {
    RedrawWindow(row, NULL, NULL, RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
}

/* Returns 0 when the handler destroyed the list. */
static int select_row(RowListData* data, int index, int by_user) {
    if (index < -1 || index >= data->count) return 1;
    if (index != data->selected) {
        if (data->selected >= 0) {
            HWND old = data->items[data->selected];
            RemovePropA(old, ROW_SELECTED_PROP);
            repaint(old);
        }
        data->selected = index;
        if (index >= 0) {
            HWND row = data->items[index];
            SetPropA(row, ROW_SELECTED_PROP, (HANDLE)1);
            repaint(row);
            ng_windows_scroll_view_reveal(data->scroll, row);
            NotifyWinEvent(EVENT_OBJECT_SELECTION, row, OBJID_CLIENT, CHILDID_SELF);
            if (GetFocus() == data->scroll) NotifyWinEvent(EVENT_OBJECT_FOCUS, row, OBJID_CLIENT, CHILDID_SELF);
        }
        if (by_user) {
            HWND scroll = data->scroll;
            ng_invoke_row_list_selected(data->id, index);
            if (!IsWindow(scroll) || list_data(scroll) != data) return 0;
        }
    }
    return 1;
}

static void activate(RowListData* data, int index) {
    if (index >= 0 && index < data->count) ng_invoke_row_list_activated(data->id, index);
}

static int row_at(RowListData* data, POINT in_scroll) {
    MapWindowPoints(data->scroll, NULL, &in_scroll, 1);
    for (int i = 0; i < data->count; i++) {
        RECT r;
        GetWindowRect(data->items[i], &r);
        if (PtInRect(&r, in_scroll)) return i;
    }
    return -1;
}

static void clicked(RowListData* data, int index, int double_click_allowed) {
    if (index < 0) return;
    HWND row = data->items[index];
    DWORD now = GetMessageTime();
    int twice = double_click_allowed && row == data->last_click &&
                now - data->last_click_time <= GetDoubleClickTime();
    data->last_click = twice ? NULL : row;
    data->last_click_time = now;
    if (!select_row(data, index, 1)) return;
    if (twice) activate(data, index);
}

static long row_state(HWND row) {
    RowListData* data = data_of_row(row);
    long state = STATE_SYSTEM_SELECTABLE | STATE_SYSTEM_FOCUSABLE;
    if (GetPropA(row, ROW_SELECTED_PROP)) {
        state |= STATE_SYSTEM_SELECTED;
        if (data && GetFocus() == data->scroll) state |= STATE_SYSTEM_FOCUSED;
    }
    return state;
}

typedef struct {
    wchar_t* out;
    int len;
    int at;
} NameParts;

static BOOL CALLBACK add_label(HWND child, LPARAM param) {
    NameParts* parts = (NameParts*)param;
    if (!GetPropA(child, BOX_LABEL_PROP) || !IsWindowVisible(child)) return TRUE;
    int room = parts->len - parts->at;
    if (room <= 3) return FALSE;
    if (parts->at > 0) {
        wcscpy_s(parts->out + parts->at, (size_t)room, L", ");
        parts->at += 2;
        room -= 2;
    }
    int got = GetWindowTextW(child, parts->out + parts->at, room);
    if (got == 0 && parts->at >= 2) parts->at -= 2;
    parts->at += got;
    parts->out[parts->at] = 0;
    return TRUE;
}

/* A row reads as its labels. */
static int row_name(HWND row, wchar_t* out, int len) {
    NameParts parts = {out, len, 0};
    out[0] = 0;
    EnumChildWindows(row, add_label, (LPARAM)&parts);
    return parts.at > 0;
}

static LRESULT CALLBACK row_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam, UINT_PTR id,
                                 DWORD_PTR ref) {
    (void)ref;
    if (msg == WM_ERASEBKGND) {
        LRESULT erased = ng_windows_def_subclass(hwnd, msg, wParam, lParam);
        RowListData* data = data_of_row(hwnd);
        if (data && GetPropA(hwnd, ROW_SELECTED_PROP) && GetFocus() == data->scroll) {
            RECT r;
            GetClientRect(hwnd, &r);
            InflateRect(&r, -1, -1);
            DrawFocusRect((HDC)wParam, &r);
        }
        return erased;
    } else if (msg == WM_NCDESTROY) {
        RemovePropA(hwnd, ROW_SELECTED_PROP);
        RemovePropA(hwnd, ROW_LIST_PROP);
        ng_windows_remove_subclass(hwnd, row_proc, id);
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

static LRESULT CALLBACK list_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam, UINT_PTR id,
                                  DWORD_PTR ref) {
    RowListData* data = (RowListData*)ref;
    switch (msg) {
        case WM_LBUTTONDOWN: {
            SetFocus(hwnd);
            POINT at = {(short)LOWORD(lParam), (short)HIWORD(lParam)};
            clicked(data, row_at(data, at), 1);
            return 0;
        }
        case WM_PARENTNOTIFY:
            if (LOWORD(wParam) == WM_LBUTTONDOWN) {
                POINT at = {(short)LOWORD(lParam), (short)HIWORD(lParam)};
                clicked(data, row_at(data, at), 0);
            }
            break;
        case WM_GETDLGCODE:
            return DLGC_WANTARROWS;
        case WM_CTLCOLORSTATIC:
        case WM_CTLCOLORBTN:
            SetBkColor((HDC)wParam, GetSysColor(COLOR_WINDOW));
            return (LRESULT)GetSysColorBrush(COLOR_WINDOW);
        case WM_KEYDOWN: {
            int at = data->selected;
            int last = data->count - 1;
            switch (wParam) {
                case VK_UP: select_row(data, at > 0 ? at - 1 : (last >= 0 ? 0 : -1), 1); return 0;
                case VK_DOWN: select_row(data, at < last ? at + 1 : last, 1); return 0;
                case VK_HOME: select_row(data, last >= 0 ? 0 : -1, 1); return 0;
                case VK_END: select_row(data, last, 1); return 0;
                case VK_RETURN: activate(data, at); return 0;
                default: break;
            }
            break;
        }
        case WM_SETFOCUS:
        case WM_KILLFOCUS:
            if (data->selected >= 0) {
                HWND row = data->items[data->selected];
                repaint(row);
                if (msg == WM_SETFOCUS) NotifyWinEvent(EVENT_OBJECT_FOCUS, row, OBJID_CLIENT, CHILDID_SELF);
            }
            break;
        case WM_NCDESTROY:
            RemovePropA(hwnd, ROW_LIST_PROP);
            free(data->items);
            free(data);
            RemovePropA(hwnd, ROW_LIST_DATA_PROP);
            ng_windows_remove_subclass(hwnd, list_proc, id);
            return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
        default:
            break;
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_row_list(unsigned int id) {
    HWND scroll = (HWND)ng_windows_create_scroll_view();
    HWND rows = (HWND)ng_windows_create_box(1);
    RowListData* data = (RowListData*)calloc(1, sizeof(RowListData));
    if (!scroll || !rows || !data) {
        if (scroll) DestroyWindow(scroll);
        if (rows) DestroyWindow(rows);
        free(data);
        return NULL;
    }
    data->id = id;
    data->scroll = scroll;
    data->rows = rows;
    data->selected = -1;
    ng_windows_box_set_padding((NGHandle)rows, 0);
    ng_windows_box_set_spacing((NGHandle)rows, 0);
    ng_windows_scroll_view_set_content((NGHandle)scroll, (NGHandle)rows);
    SetPropA(scroll, ROW_LIST_DATA_PROP, (HANDLE)data);
    SetPropA(scroll, ROW_LIST_PROP, (HANDLE)1);
    SetPropA(rows, ROW_LIST_PROP, (HANDLE)1);
    if (!ng_windows_set_subclass(scroll, list_proc, ROW_LIST_SUBCLASS_ID, (DWORD_PTR)data)) {
        RemovePropA(scroll, ROW_LIST_DATA_PROP);
        free(data);
        DestroyWindow(scroll);
        return NULL;
    }
    /* One tab stop: Tab lands on the list, arrows move inside it. */
    SetWindowLongPtrW(scroll, GWL_STYLE, GetWindowLongPtrW(scroll, GWL_STYLE) | WS_TABSTOP);
    SetWindowLongPtrW(scroll, GWL_EXSTYLE,
                      (GetWindowLongPtrW(scroll, GWL_EXSTYLE) & ~WS_EX_CONTROLPARENT) | WS_EX_CLIENTEDGE);
    SetWindowPos(scroll, NULL, 0, 0, 300, 200, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    ng_windows_set_role(scroll, ROLE_SYSTEM_LIST);
    return (NGHandle)scroll;
}

static RowListData* data_of(NGHandle list) {
    return list ? list_data((HWND)list) : NULL;
}

int ng_windows_row_list_insert(NGHandle list, int index, NGHandle element) {
    RowListData* data = data_of(list);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (!element || !IsWindow((HWND)element) || index < 0 || index > data->count) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    if (data->count == data->capacity) {
        int capacity = data->capacity ? data->capacity * 2 : 8;
        HWND* grown = (HWND*)realloc(data->items, (size_t)capacity * sizeof(HWND));
        if (!grown) return NG_ERROR_CREATION_FAILED;
        data->items = grown;
        data->capacity = capacity;
    }
    HWND row = (HWND)ng_windows_create_box(1);
    if (!row) return NG_ERROR_CREATION_FAILED;
    if (!ng_windows_set_subclass(row, row_proc, ROW_SUBCLASS_ID, 0)) {
        DestroyWindow(row);
        return NG_ERROR_CREATION_FAILED;
    }
    ng_windows_box_set_padding((NGHandle)row, ROW_PADDING);
    int added = ng_windows_box_add((NGHandle)row, element);
    if (added != NG_SUCCESS) {
        DestroyWindow(row);
        return added;
    }
    ng_windows_set_role(row, ROLE_SYSTEM_LISTITEM);
    ng_windows_serve_state(row, row_state);
    ng_windows_serve_name(row, row_name);
    ng_windows_box_add((NGHandle)data->rows, (NGHandle)row);
    memmove(data->items + index + 1, data->items + index, (size_t)(data->count - index) * sizeof(HWND));
    data->items[index] = row;
    data->count++;
    if (data->selected >= index) data->selected++;
    relayout(data);
    return NG_SUCCESS;
}

int ng_windows_row_list_remove(NGHandle list, int index) {
    RowListData* data = data_of(list);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= data->count) return NG_ERROR_INVALID_PARAMETER;
    HWND row = data->items[index];
    memmove(data->items + index, data->items + index + 1, (size_t)(data->count - index - 1) * sizeof(HWND));
    data->count--;
    if (data->selected == index) data->selected = -1;
    else if (data->selected > index) data->selected--;
    if (data->last_click == row) data->last_click = NULL;
    DestroyWindow(row);
    relayout(data);
    return NG_SUCCESS;
}

int ng_windows_row_list_clear(NGHandle list) {
    RowListData* data = data_of(list);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    while (data->count > 0) {
        HWND row = data->items[--data->count];
        DestroyWindow(row);
    }
    data->selected = -1;
    data->last_click = NULL;
    relayout(data);
    return NG_SUCCESS;
}

int ng_windows_row_list_set_selected(NGHandle list, int index) {
    RowListData* data = data_of(list);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= data->count) return NG_ERROR_INVALID_PARAMETER;
    select_row(data, index, 0);
    return NG_SUCCESS;
}

int ng_windows_row_list_get_selected(NGHandle list) {
    RowListData* data = data_of(list);
    return data ? data->selected : -1;
}
