#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include "../utils.h"
#include <windows.h>
#include <stdlib.h>

#define TABBAR_OLD_PROC_PROP "AureaTabBarOldProc"
#define TABBAR_DATA_PROP "AureaTabBarData"
#define TABBAR_BASE_ID 9000
#define TAB_MIN_HEIGHT 28
#define TAB_TEXT_PADDING 24

typedef struct {
    unsigned int id;
    int count;
    int selected;
} TabBarData;

static TabBarData* tab_data(HWND bar) {
    return (TabBarData*)GetPropA(bar, TABBAR_DATA_PROP);
}

static HWND tab_button(HWND bar, int index) {
    return GetDlgItem(bar, TABBAR_BASE_ID + index);
}

static int tab_width(HWND btn) {
    HDC hdc = GetDC(btn);
    if (!hdc) return ng_windows_px(btn, BUTTON_MIN_WIDTH);
    HFONT old = (HFONT)SelectObject(hdc, (HFONT)SendMessageW(btn, WM_GETFONT, 0, 0));
    int len = GetWindowTextLengthW(btn);
    wchar_t* text = (wchar_t*)malloc(((size_t)len + 1) * sizeof(wchar_t));
    SIZE size = {0, 0};
    if (text) {
        len = GetWindowTextW(btn, text, len + 1);
        GetTextExtentPoint32W(hdc, text, len, &size);
        free(text);
    }
    SelectObject(hdc, old);
    ReleaseDC(btn, hdc);
    return size.cx + ng_windows_px(btn, TAB_TEXT_PADDING);
}

int ng_windows_tab_bar_measure(HWND bar, int* width, int* height) {
    TabBarData* data = tab_data(bar);
    if (!data) return 0;
    int w = 0;
    for (int i = 0; i < data->count; i++) w += tab_width(tab_button(bar, i));
    HDC hdc = GetDC(bar);
    TEXTMETRICW tm = {0};
    if (hdc) {
        HFONT old = (HFONT)SelectObject(hdc, ng_windows_ui_font(bar));
        GetTextMetricsW(hdc, &tm);
        SelectObject(hdc, old);
        ReleaseDC(bar, hdc);
    }
    int h = tm.tmHeight + ng_windows_px(bar, 12);
    int min_h = ng_windows_px(bar, TAB_MIN_HEIGHT);
    *width = w;
    *height = h > min_h ? h : min_h;
    return 1;
}

static void tab_bar_layout(HWND bar) {
    TabBarData* data = tab_data(bar);
    if (!data) return;
    RECT client;
    GetClientRect(bar, &client);
    int x = 0;
    for (int i = 0; i < data->count; i++) {
        HWND btn = tab_button(bar, i);
        int w = tab_width(btn);
        SetWindowPos(btn, NULL, x, 0, w, client.bottom, SWP_NOZORDER | SWP_NOACTIVATE);
        x += w;
    }
}

static void tab_bar_show_selected(HWND bar) {
    TabBarData* data = tab_data(bar);
    for (int i = 0; i < data->count; i++) {
        SendMessageW(tab_button(bar, i), BM_SETCHECK,
                     i == data->selected ? BST_CHECKED : BST_UNCHECKED, 0);
    }
}

static LRESULT CALLBACK TabBarProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    TabBarData* data = tab_data(hwnd);
    if (msg == WM_COMMAND && data && HIWORD(wParam) == BN_CLICKED) {
        int idx = (int)LOWORD(wParam) - TABBAR_BASE_ID;
        if (idx >= 0 && idx < data->count) {
            if (idx != data->selected) {
                data->selected = idx;
                tab_bar_show_selected(hwnd);
                ng_invoke_tab_bar_selected(data->id, idx);
            }
            return 0;
        }
    } else if (msg == WM_SIZE) {
        tab_bar_layout(hwnd);
    } else if (msg == WM_ERASEBKGND) {
        RECT client;
        GetClientRect(hwnd, &client);
        FillRect((HDC)wParam, &client, GetSysColorBrush(COLOR_BTNFACE));
        return 1;
    } else if (msg == WM_NCDESTROY) {
        free(data);
        RemovePropA(hwnd, TABBAR_DATA_PROP);
    }

    WNDPROC old_proc = (WNDPROC)GetPropA(hwnd, TABBAR_OLD_PROC_PROP);
    if (msg == WM_NCDESTROY) RemovePropA(hwnd, TABBAR_OLD_PROC_PROP);
    if (old_proc) {
        return CallWindowProcA(old_proc, hwnd, msg, wParam, lParam);
    }
    return DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_tab_bar(unsigned int id) {
    HWND temp_parent = ng_windows_detached_parent();
    HWND container = CreateWindowExA(
        WS_EX_CONTROLPARENT,
        "STATIC",
        NULL,
        WS_CHILD | SS_LEFT | WS_VISIBLE | WS_CLIPCHILDREN,
        0, 0, 100, ng_windows_px(temp_parent, TAB_MIN_HEIGHT),
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    if (!container) return NULL;

    TabBarData* data = (TabBarData*)calloc(1, sizeof(TabBarData));
    if (!data) {
        DestroyWindow(container);
        return NULL;
    }
    data->id = id;
    data->selected = -1;
    SetPropA(container, TABBAR_DATA_PROP, (HANDLE)data);
    WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(container, GWLP_WNDPROC, (LONG_PTR)TabBarProc);
    if (old_proc) {
        SetPropA(container, TABBAR_OLD_PROC_PROP, (HANDLE)old_proc);
    }
    return (NGHandle)container;
}

int ng_windows_tab_bar_add_tab(NGHandle tab_bar, const char* title) {
    if (!tab_bar || !title) return NG_ERROR_INVALID_PARAMETER;
    HWND bar = (HWND)tab_bar;
    TabBarData* data = tab_data(bar);
    if (!data) return NG_ERROR_INVALID_HANDLE;

    char* shown = ng_windows_escape_mnemonics(title);
    if (!shown) return NG_ERROR_INVALID_PARAMETER;
    HWND btn = ng_windows_create_control(
        0, L"BUTTON", shown, WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_CHECKBOX | BS_PUSHLIKE,
        80, 24, bar, TABBAR_BASE_ID + data->count);
    free(shown);
    if (!btn) return NG_ERROR_CREATION_FAILED;
    SendMessageW(btn, WM_SETFONT, (WPARAM)ng_windows_ui_font(bar), FALSE);

    data->count++;
    if (data->selected < 0) data->selected = 0;
    tab_bar_show_selected(bar);
    tab_bar_layout(bar);
    ng_windows_relayout(bar);
    return NG_SUCCESS;
}

int ng_windows_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    if (!tab_bar) return NG_ERROR_INVALID_HANDLE;
    HWND bar = (HWND)tab_bar;
    TabBarData* data = tab_data(bar);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= data->count) return NG_ERROR_INVALID_PARAMETER;

    DestroyWindow(tab_button(bar, index));
    for (int i = index + 1; i < data->count; i++) {
        SetWindowLongPtrW(tab_button(bar, i), GWLP_ID, TABBAR_BASE_ID + i - 1);
    }
    data->count--;
    if (data->selected > index || data->selected >= data->count) data->selected--;
    if (data->selected < 0 && data->count > 0) data->selected = 0;
    tab_bar_show_selected(bar);
    tab_bar_layout(bar);
    ng_windows_relayout(bar);
    return NG_SUCCESS;
}

int ng_windows_tab_bar_set_selected(NGHandle tab_bar, int index) {
    if (!tab_bar) return NG_ERROR_INVALID_HANDLE;
    TabBarData* data = tab_data((HWND)tab_bar);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= data->count) return NG_ERROR_INVALID_PARAMETER;
    data->selected = index;
    tab_bar_show_selected((HWND)tab_bar);
    return NG_SUCCESS;
}

int ng_windows_tab_bar_get_selected(NGHandle tab_bar) {
    if (!tab_bar) return -1;
    TabBarData* data = tab_data((HWND)tab_bar);
    return data ? data->selected : -1;
}

void ng_windows_tab_bar_invalidate(NGHandle tab_bar) {
    if (!tab_bar) return;
    InvalidateRect((HWND)tab_bar, NULL, FALSE);
}
