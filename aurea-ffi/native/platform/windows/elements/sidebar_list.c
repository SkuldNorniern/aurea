#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <stdlib.h>

#define SIDEBAR_SUBCLASS_ID 1
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
} SidebarState;

static LRESULT CALLBACK sidebar_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                     UINT_PTR id, DWORD_PTR data);

static SidebarState* sidebar_state(HWND hwnd) {
    DWORD_PTR data = 0;
    return ng_windows_get_subclass(hwnd, sidebar_proc, SIDEBAR_SUBCLASS_ID, &data)
               ? (SidebarState*)data
               : NULL;
}

/* Rows of the list are sections (ROW_SECTION) or items; a missing row reads as a section. */
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
    FillRect(hdc, &r, GetSysColorBrush(COLOR_BTNFACE));

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

/* A section row is not selectable: it puts the selection back on the current item. */
static void on_list_select(SidebarState* s) {
    int row = (int)SendMessageW(s->list, LB_GETCURSEL, 0, 0);
    UINT_PTR v = row_data(s->list, row);
    if (v & ROW_SECTION) {
        SendMessageW(s->list, LB_SETCURSEL,
                     (WPARAM)(s->selected >= 0 ? row_of_item(s->list, s->selected) : -1), 0);
        return;
    }
    int item = ROW_ITEM(v);
    if (item == s->selected) return;
    s->selected = item;
    /* The handler may destroy this list and free `s`; nothing touches it after. */
    ng_invoke_sidebar_list_selected(s->id, item);
}

static LRESULT CALLBACK sidebar_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                     UINT_PTR id, DWORD_PTR data) {
    SidebarState* s = (SidebarState*)data;
    switch (msg) {
    case WM_DRAWITEM:
        draw_row(hwnd, (const DRAWITEMSTRUCT*)lParam);
        return TRUE;
    case WM_COMMAND:
        if (s->list && (HWND)lParam == s->list && HIWORD(wParam) == LBN_SELCHANGE) {
            on_list_select(s);
            return 0;
        }
        break;
    case WM_SIZE:
        if (s->list) SetWindowPos(s->list, NULL, 0, 0, LOWORD(lParam), HIWORD(lParam),
                                  SWP_NOZORDER | SWP_NOACTIVATE);
        break;
    case WM_SETFONT:
        if (s->list) SendMessageW(s->list, WM_SETFONT, wParam, lParam);
        break;
    case WM_CTLCOLORLISTBOX:
        return (LRESULT)GetSysColorBrush(COLOR_BTNFACE);
    case WM_ERASEBKGND:
        return 1;
    case WM_NCDESTROY:
        ng_windows_remove_subclass(hwnd, sidebar_proc, id);
        RemovePropA(hwnd, SIDEBAR_DATA_PROP);
        free(s);
        break;
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_sidebar_list(unsigned int id) {
    HWND container = CreateWindowExA(
        WS_EX_CONTROLPARENT,
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

    SidebarState* s = (SidebarState*)calloc(1, sizeof(SidebarState));
    if (!s) {
        DestroyWindow(container);
        return NULL;
    }
    s->id = id;
    s->selected = -1;
    if (!ng_windows_set_subclass(container, sidebar_proc, SIDEBAR_SUBCLASS_ID, (DWORD_PTR)s)) {
        free(s);
        DestroyWindow(container);
        return NULL;
    }
    SetPropA(container, SIDEBAR_DATA_PROP, (HANDLE)s);
    ng_windows_mark_container(container);

    s->list = CreateWindowExW(
        0, L"LISTBOX", NULL,
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_TABSTOP | LBS_NOTIFY | LBS_OWNERDRAWVARIABLE |
            LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT,
        0, 0, 200, 400, container, (HMENU)(UINT_PTR)SIDEBAR_LIST_ID, GetModuleHandleW(NULL), NULL);
    if (!s->list) {
        DestroyWindow(container);
        return NULL;
    }
    SendMessageW(s->list, WM_SETFONT, (WPARAM)ng_windows_ui_font(container), FALSE);
    return (NGHandle)container;
}

static int sidebar_add_row(HWND hwnd, SidebarState* s, const char* title, UINT_PTR v, int height) {
    wchar_t* wide = ng_windows_utf8_to_wide(title);
    if (!wide) return NG_ERROR_INVALID_PARAMETER;
    LRESULT row = SendMessageW(s->list, LB_ADDSTRING, 0, (LPARAM)wide);
    free(wide);
    if (row == LB_ERR || row == LB_ERRSPACE) return NG_ERROR_CREATION_FAILED;
    SendMessageW(s->list, LB_SETITEMDATA, (WPARAM)row, (LPARAM)v);
    SendMessageW(s->list, LB_SETITEMHEIGHT, (WPARAM)row, (LPARAM)ng_windows_px(hwnd, height));
    return NG_SUCCESS;
}

int ng_windows_sidebar_list_add_section(NGHandle sidebar, const char* title) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    HWND hwnd = (HWND)sidebar;
    SidebarState* s = sidebar_state(hwnd);
    if (!s) return NG_ERROR_INVALID_HANDLE;
    return sidebar_add_row(hwnd, s, title, ROW_SECTION, SECTION_HEIGHT);
}

int ng_windows_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    HWND hwnd = (HWND)sidebar;
    SidebarState* s = sidebar_state(hwnd);
    if (!s) return NG_ERROR_INVALID_HANDLE;
    if (s->item_count > 0xFFFF) return NG_ERROR_LIMIT_EXCEEDED;
    UINT_PTR depth = (UINT_PTR)(indent > 0 ? (indent > 0x7FFF ? 0x7FFF : indent) : 0);
    int result = sidebar_add_row(hwnd, s, title, (depth << 16) | (UINT_PTR)s->item_count, ITEM_HEIGHT);
    if (result == NG_SUCCESS) s->item_count++;
    return result;
}

int ng_windows_sidebar_list_set_selected(NGHandle sidebar, int index) {
    SidebarState* s = sidebar ? sidebar_state((HWND)sidebar) : NULL;
    if (!s) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= s->item_count) return NG_ERROR_INVALID_PARAMETER;
    s->selected = index;
    SendMessageW(s->list, LB_SETCURSEL, (WPARAM)(index >= 0 ? row_of_item(s->list, index) : -1), 0);
    return NG_SUCCESS;
}

int ng_windows_sidebar_list_get_selected(NGHandle sidebar) {
    SidebarState* s = sidebar ? sidebar_state((HWND)sidebar) : NULL;
    return s ? s->selected : -1;
}

int ng_windows_sidebar_list_clear(NGHandle sidebar) {
    SidebarState* s = sidebar ? sidebar_state((HWND)sidebar) : NULL;
    if (!s) return NG_ERROR_INVALID_HANDLE;
    SendMessageW(s->list, LB_RESETCONTENT, 0, 0);
    s->item_count = 0;
    s->selected = -1;
    return NG_SUCCESS;
}

void ng_windows_sidebar_list_invalidate(NGHandle sidebar) {
    SidebarState* s = sidebar ? sidebar_state((HWND)sidebar) : NULL;
    if (s) InvalidateRect(s->list, NULL, FALSE);
}
