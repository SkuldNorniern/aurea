#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <commctrl.h>
#include <stdlib.h>

#define TABLE_OLD_PROC_PROP "AureaTableOldProc"
#define TABLE_LIST_ID 1
/* Posted so a selection moving from one row to another reports once, after both halves. */
#define WM_TABLE_SELECTION (WM_APP + 0x7A)

typedef struct {
    unsigned int id;
    HWND list;
    int columns;
    int selected;
    int pending;
} TableData;

static TableData* table_data(HWND table) {
    return (TableData*)GetPropA(table, TABLE_DATA_PROP);
}

static int list_selected(HWND list) {
    return (int)SendMessageW(list, LVM_GETNEXTITEM, (WPARAM)-1, LVNI_SELECTED);
}

static void fit_columns(TableData* data) {
    if (data->columns == 0) return;
    RECT client;
    GetClientRect(data->list, &client);
    int each = client.right / data->columns;
    for (int i = 0; i < data->columns; i++) {
        int width = i == data->columns - 1 ? client.right - each * i : each;
        SendMessageW(data->list, LVM_SETCOLUMNWIDTH, (WPARAM)i, MAKELPARAM(width, 0));
    }
}

static LRESULT CALLBACK TableProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    TableData* data = table_data(hwnd);
    if (data && msg == WM_NOTIFY && ((NMHDR*)lParam)->hwndFrom == data->list) {
        NMHDR* header = (NMHDR*)lParam;
        if (header->code == LVN_ITEMCHANGED && !data->pending) {
            data->pending = 1;
            PostMessageW(hwnd, WM_TABLE_SELECTION, 0, 0);
        } else if (header->code == LVN_ITEMACTIVATE) {
            int row = ((NMITEMACTIVATE*)lParam)->iItem;
            /* The handler may destroy this table. */
            if (row >= 0) ng_invoke_table_activated(data->id, row);
            return 0;
        }
    } else if (data && msg == WM_TABLE_SELECTION) {
        data->pending = 0;
        int row = list_selected(data->list);
        if (row != data->selected) {
            data->selected = row;
            ng_invoke_table_selected(data->id, row);
        }
        return 0;
    } else if (data && msg == WM_SIZE) {
        SetWindowPos(data->list, NULL, 0, 0, LOWORD(lParam), HIWORD(lParam), SWP_NOZORDER | SWP_NOACTIVATE);
        fit_columns(data);
    } else if (data && msg == WM_SETFONT) {
        SendMessageW(data->list, WM_SETFONT, wParam, lParam);
    } else if (msg == WM_ERASEBKGND) {
        return 1;
    } else if (msg == WM_NCDESTROY) {
        free(data);
        RemovePropA(hwnd, TABLE_DATA_PROP);
    }

    WNDPROC old_proc = (WNDPROC)GetPropA(hwnd, TABLE_OLD_PROC_PROP);
    if (msg == WM_NCDESTROY) RemovePropA(hwnd, TABLE_OLD_PROC_PROP);
    return old_proc ? CallWindowProcA(old_proc, hwnd, msg, wParam, lParam)
                    : DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_table(unsigned int id) {
    HWND container = CreateWindowExA(WS_EX_CONTROLPARENT, "STATIC", NULL,
                                     WS_CHILD | SS_LEFT | WS_VISIBLE | WS_CLIPCHILDREN,
                                     0, 0, 300, 200, ng_windows_detached_parent(), NULL,
                                     GetModuleHandleA(NULL), NULL);
    if (!container) return NULL;
    TableData* data = (TableData*)calloc(1, sizeof(TableData));
    if (!data) {
        DestroyWindow(container);
        return NULL;
    }
    data->id = id;
    data->selected = -1;
    SetPropA(container, TABLE_DATA_PROP, (HANDLE)data);
    WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(container, GWLP_WNDPROC, (LONG_PTR)TableProc);
    if (old_proc) {
        SetPropA(container, TABLE_OLD_PROC_PROP, (HANDLE)old_proc);
        ng_windows_mark_container(container);
    }

    ng_windows_init_common_controls(ICC_LISTVIEW_CLASSES);
    data->list = CreateWindowExW(
        WS_EX_CLIENTEDGE, WC_LISTVIEWW, NULL,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS,
        0, 0, 300, 200, container, (HMENU)(UINT_PTR)TABLE_LIST_ID, GetModuleHandleW(NULL), NULL);
    if (!data->list) {
        DestroyWindow(container);
        return NULL;
    }
    SendMessageW(data->list, LVM_SETEXTENDEDLISTVIEWSTYLE, 0,
                 LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_LABELTIP);
    SendMessageW(data->list, WM_SETFONT, (WPARAM)ng_windows_ui_font(container), FALSE);
    return (NGHandle)container;
}

static TableData* data_of(NGHandle table) {
    return table ? table_data((HWND)table) : NULL;
}

static int row_count(TableData* data) {
    return (int)SendMessageW(data->list, LVM_GETITEMCOUNT, 0, 0);
}

int ng_windows_table_add_column(NGHandle table, const char* title) {
    TableData* data = data_of(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (!title) return NG_ERROR_INVALID_PARAMETER;
    if (row_count(data) > 0) return NG_ERROR_INVALID_PARAMETER;
    wchar_t* wide = ng_windows_utf8_to_wide(title);
    if (!wide) return NG_ERROR_INVALID_PARAMETER;
    LVCOLUMNW column = {0};
    column.mask = LVCF_TEXT | LVCF_SUBITEM;
    column.pszText = wide;
    column.iSubItem = data->columns;
    LRESULT added = SendMessageW(data->list, LVM_INSERTCOLUMNW, (WPARAM)data->columns, (LPARAM)&column);
    free(wide);
    if (added < 0) return NG_ERROR_CREATION_FAILED;
    data->columns++;
    fit_columns(data);
    return NG_SUCCESS;
}

static int set_cells(TableData* data, int row, const char* const* cells, int count) {
    for (int i = 0; i < data->columns; i++) {
        wchar_t* wide = ng_windows_utf8_to_wide(i < count && cells[i] ? cells[i] : "");
        if (!wide) return NG_ERROR_INVALID_PARAMETER;
        LVITEMW item = {0};
        item.iSubItem = i;
        item.pszText = wide;
        SendMessageW(data->list, LVM_SETITEMTEXTW, (WPARAM)row, (LPARAM)&item);
        free(wide);
    }
    return NG_SUCCESS;
}

static int check_cells(TableData* data, const char* const* cells, int count) {
    if (count < 0 || count > data->columns || (count > 0 && !cells)) return NG_ERROR_INVALID_PARAMETER;
    return NG_SUCCESS;
}

/* After a change made by the app, so the next notification does not report it. */
static void settle_selection(TableData* data) {
    data->selected = list_selected(data->list);
}

int ng_windows_table_insert_row(NGHandle table, int index, const char* const* cells, int count) {
    TableData* data = data_of(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index > row_count(data)) return NG_ERROR_INVALID_PARAMETER;
    int checked = check_cells(data, cells, count);
    if (checked != NG_SUCCESS) return checked;
    LVITEMW item = {0};
    item.mask = LVIF_TEXT;
    item.iItem = index;
    item.pszText = L"";
    LRESULT row = SendMessageW(data->list, LVM_INSERTITEMW, 0, (LPARAM)&item);
    if (row < 0) return NG_ERROR_CREATION_FAILED;
    settle_selection(data);
    return set_cells(data, (int)row, cells, count);
}

int ng_windows_table_set_row(NGHandle table, int index, const char* const* cells, int count) {
    TableData* data = data_of(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= row_count(data)) return NG_ERROR_INVALID_PARAMETER;
    int checked = check_cells(data, cells, count);
    return checked != NG_SUCCESS ? checked : set_cells(data, index, cells, count);
}

int ng_windows_table_remove_row(NGHandle table, int index) {
    TableData* data = data_of(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= row_count(data)) return NG_ERROR_INVALID_PARAMETER;
    SendMessageW(data->list, LVM_DELETEITEM, (WPARAM)index, 0);
    settle_selection(data);
    return NG_SUCCESS;
}

int ng_windows_table_clear(NGHandle table) {
    TableData* data = data_of(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    SendMessageW(data->list, LVM_DELETEALLITEMS, 0, 0);
    settle_selection(data);
    return NG_SUCCESS;
}

int ng_windows_table_set_selected(NGHandle table, int index) {
    TableData* data = data_of(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= row_count(data)) return NG_ERROR_INVALID_PARAMETER;
    LVITEMW clear = {0};
    clear.stateMask = LVIS_SELECTED;
    SendMessageW(data->list, LVM_SETITEMSTATE, (WPARAM)-1, (LPARAM)&clear);
    if (index >= 0) {
        LVITEMW item = {0};
        item.stateMask = LVIS_SELECTED | LVIS_FOCUSED;
        item.state = LVIS_SELECTED | LVIS_FOCUSED;
        SendMessageW(data->list, LVM_SETITEMSTATE, (WPARAM)index, (LPARAM)&item);
        SendMessageW(data->list, LVM_ENSUREVISIBLE, (WPARAM)index, FALSE);
    }
    settle_selection(data);
    return NG_SUCCESS;
}

int ng_windows_table_get_selected(NGHandle table) {
    TableData* data = data_of(table);
    return data ? list_selected(data->list) : -1;
}
