#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <stdlib.h>

#define SIDEBAR_OLD_PROC_PROP "AureaSidebarOldProc"
#define SIDEBAR_DATA_PROP "AureaSidebarData"
#define SIDEBAR_LIST_ID 1
#define ITEM_HEIGHT 30
#define SECTION_HEIGHT 28
#define SIDE_MARGIN 6
#define TEXT_INSET 14
#define INDENT_STEP 14
#define ACCENT_WIDTH 3

/* Packed into each row's item data: kind in the top bit, indent, item index. */
#define ROW_SECTION 0x80000000u
#define ROW_INDENT(v) (((v) >> 16) & 0x7FFF)
#define ROW_ITEM(v) ((int)((v) & 0xFFFF))

typedef struct {
    unsigned int id;
    HWND list;
    int item_count;
    int selected;
} SidebarData;

static SidebarData* sidebar_data(HWND bar) {
    return (SidebarData*)GetPropA(bar, SIDEBAR_DATA_PROP);
}

static UINT_PTR row_data(HWND list, int row) {
    LRESULT v = SendMessageW(list, LB_GETITEMDATA, (WPARAM)row, 0);
    return v == LB_ERR ? ROW_SECTION : (UINT_PTR)v;
}

static int row_of_item(HWND list, int item) {
    int count = (int)SendMessageW(list, LB_GETCOUNT, 0, 0);
    for (int row = 0; row < count; row++) {
        UINT_PTR v = row_data(list, row);
        if (!(v & ROW_SECTION) && ROW_ITEM(v) == item) return row;
    }
    return -1;
}

static COLORREF mix(COLORREF a, COLORREF b, int b_percent) {
    int keep = 100 - b_percent;
    return RGB((GetRValue(a) * keep + GetRValue(b) * b_percent) / 100,
               (GetGValue(a) * keep + GetGValue(b) * b_percent) / 100,
               (GetBValue(a) * keep + GetBValue(b) * b_percent) / 100);
}

static void fill_round(HDC hdc, RECT r, int radius, COLORREF color) {
    HBRUSH brush = CreateSolidBrush(color);
    HGDIOBJ old_brush = SelectObject(hdc, brush);
    HGDIOBJ old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
    RoundRect(hdc, r.left, r.top, r.right + 1, r.bottom + 1, radius, radius);
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(brush);
}

static void draw_row(HWND bar, const DRAWITEMSTRUCT* item) {
    if (item->itemID == (UINT)-1) return;
    HDC hdc = item->hDC;
    RECT r = item->rcItem;
    COLORREF face = GetSysColor(COLOR_BTNFACE);
    HBRUSH bg = CreateSolidBrush(face);
    FillRect(hdc, &r, bg);
    DeleteObject(bg);

    UINT_PTR v = (UINT_PTR)item->itemData;
    int section = (v & ROW_SECTION) != 0;
    int selected = !section && (item->itemState & ODS_SELECTED);
    COLORREF accent = GetSysColor(COLOR_HIGHLIGHT);
    if (selected) {
        RECT pill = r;
        InflateRect(&pill, -ng_windows_px(bar, SIDE_MARGIN), -ng_windows_px(bar, 2));
        fill_round(hdc, pill, ng_windows_px(bar, 8), mix(face, accent, 18));
        RECT bar_rect = pill;
        bar_rect.right = bar_rect.left + ng_windows_px(bar, ACCENT_WIDTH);
        InflateRect(&bar_rect, 0, -ng_windows_px(bar, 7));
        fill_round(hdc, bar_rect, ng_windows_px(bar, 3), accent);
    }

    int len = (int)SendMessageW(item->hwndItem, LB_GETTEXTLEN, item->itemID, 0);
    if (len <= 0) return;
    wchar_t* text = (wchar_t*)malloc(((size_t)len + 1) * sizeof(wchar_t));
    if (!text) return;
    SendMessageW(item->hwndItem, LB_GETTEXT, item->itemID, (LPARAM)text);

    RECT t = r;
    t.left += ng_windows_px(bar, SIDE_MARGIN + TEXT_INSET + (section ? 0 : (int)ROW_INDENT(v) * INDENT_STEP));
    t.right -= ng_windows_px(bar, SIDE_MARGIN);
    if (section) t.top += ng_windows_px(bar, 6);
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, GetSysColor(section ? COLOR_GRAYTEXT : COLOR_BTNTEXT));
    HGDIOBJ old_font = SelectObject(hdc, ng_windows_ui_font(bar));
    DrawTextW(hdc, text, len, &t, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
    SelectObject(hdc, old_font);
    free(text);
}

static void sidebar_select_row(SidebarData* data, int row) {
    SendMessageW(data->list, LB_SETCURSEL, (WPARAM)row, 0);
}

static LRESULT CALLBACK SidebarProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    SidebarData* data = sidebar_data(hwnd);
    if (data && msg == WM_MEASUREITEM) {
        MEASUREITEMSTRUCT* m = (MEASUREITEMSTRUCT*)lParam;
        int section = (m->itemData & ROW_SECTION) != 0;
        m->itemHeight = (UINT)ng_windows_px(hwnd, section ? SECTION_HEIGHT : ITEM_HEIGHT);
        return TRUE;
    } else if (data && msg == WM_DRAWITEM) {
        draw_row(hwnd, (const DRAWITEMSTRUCT*)lParam);
        return TRUE;
    } else if (data && msg == WM_COMMAND && (HWND)lParam == data->list && HIWORD(wParam) == LBN_SELCHANGE) {
        int row = (int)SendMessageW(data->list, LB_GETCURSEL, 0, 0);
        UINT_PTR v = row_data(data->list, row);
        if (v & ROW_SECTION) {
            sidebar_select_row(data, data->selected >= 0 ? row_of_item(data->list, data->selected) : -1);
            return 0;
        }
        int item = ROW_ITEM(v);
        if (item != data->selected) {
            data->selected = item;
            /* The handler may destroy this list. */
            ng_invoke_sidebar_list_selected(data->id, item);
        }
        return 0;
    } else if (data && msg == WM_SIZE) {
        SetWindowPos(data->list, NULL, 0, 0, LOWORD(lParam), HIWORD(lParam), SWP_NOZORDER | SWP_NOACTIVATE);
    } else if (data && msg == WM_SETFONT) {
        SendMessageW(data->list, WM_SETFONT, wParam, lParam);
    } else if (data && msg == WM_CTLCOLORLISTBOX) {
        return (LRESULT)GetSysColorBrush(COLOR_BTNFACE);
    } else if (msg == WM_ERASEBKGND) {
        return 1;
    } else if (msg == WM_NCDESTROY) {
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
        WS_CHILD | SS_LEFT | WS_VISIBLE | WS_CLIPCHILDREN,
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

    data->list = CreateWindowExW(
        0, L"LISTBOX", NULL,
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_TABSTOP | LBS_NOTIFY | LBS_OWNERDRAWVARIABLE |
            LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT,
        0, 0, 200, 400, container, (HMENU)(UINT_PTR)SIDEBAR_LIST_ID, GetModuleHandleW(NULL), NULL);
    if (!data->list) {
        DestroyWindow(container);
        return NULL;
    }
    SendMessageW(data->list, WM_SETFONT, (WPARAM)ng_windows_ui_font(container), FALSE);
    return (NGHandle)container;
}

static int sidebar_add_row(SidebarData* data, const char* title, UINT_PTR v) {
    wchar_t* wide = ng_windows_utf8_to_wide(title);
    if (!wide) return NG_ERROR_INVALID_PARAMETER;
    LRESULT row = SendMessageW(data->list, LB_ADDSTRING, 0, (LPARAM)wide);
    free(wide);
    if (row == LB_ERR || row == LB_ERRSPACE) return NG_ERROR_CREATION_FAILED;
    SendMessageW(data->list, LB_SETITEMDATA, (WPARAM)row, (LPARAM)v);
    return NG_SUCCESS;
}

int ng_windows_sidebar_list_add_section(NGHandle sidebar, const char* title) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    SidebarData* data = sidebar_data((HWND)sidebar);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    return sidebar_add_row(data, title, ROW_SECTION);
}

int ng_windows_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    SidebarData* data = sidebar_data((HWND)sidebar);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (data->item_count > 0xFFFF) return NG_ERROR_LIMIT_EXCEEDED;
    UINT_PTR depth = (UINT_PTR)(indent > 0 ? (indent > 0x7FFF ? 0x7FFF : indent) : 0);
    int result = sidebar_add_row(data, title, (depth << 16) | (UINT_PTR)data->item_count);
    if (result == NG_SUCCESS) data->item_count++;
    return result;
}

int ng_windows_sidebar_list_set_selected(NGHandle sidebar, int index) {
    SidebarData* data = sidebar ? sidebar_data((HWND)sidebar) : NULL;
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= data->item_count) return NG_ERROR_INVALID_PARAMETER;
    data->selected = index;
    sidebar_select_row(data, index >= 0 ? row_of_item(data->list, index) : -1);
    return NG_SUCCESS;
}

int ng_windows_sidebar_list_get_selected(NGHandle sidebar) {
    SidebarData* data = sidebar ? sidebar_data((HWND)sidebar) : NULL;
    return data ? data->selected : -1;
}

int ng_windows_sidebar_list_clear(NGHandle sidebar) {
    SidebarData* data = sidebar ? sidebar_data((HWND)sidebar) : NULL;
    if (!data) return NG_ERROR_INVALID_HANDLE;
    SendMessageW(data->list, LB_RESETCONTENT, 0, 0);
    data->item_count = 0;
    data->selected = -1;
    return NG_SUCCESS;
}

void ng_windows_sidebar_list_invalidate(NGHandle sidebar) {
    SidebarData* data = sidebar ? sidebar_data((HWND)sidebar) : NULL;
    if (data) InvalidateRect(data->list, NULL, FALSE);
}
