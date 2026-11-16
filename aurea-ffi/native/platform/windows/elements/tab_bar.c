#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include "../utils.h"
#include <windows.h>
#include <stdlib.h>
#include <oleacc.h>

#define TABBAR_SUBCLASS_ID 1
#define TABBAR_BASE_ID 9000
#define TAB_MIN_HEIGHT 28
#define TAB_TEXT_PADDING 24

typedef struct {
    unsigned int id;
    int count;
    int selected;
} TabBarState;

static LRESULT CALLBACK tab_bar_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                     UINT_PTR id, DWORD_PTR data);

static TabBarState* bar_state(HWND bar) {
    DWORD_PTR data = 0;
    return bar && ng_windows_get_subclass(bar, tab_bar_proc, TABBAR_SUBCLASS_ID, &data)
               ? (TabBarState*)data
               : NULL;
}

static HWND tab_button(HWND bar, int index) {
    return GetDlgItem(bar, TABBAR_BASE_ID + index);
}

static int tab_width(HWND btn) {
    if (!btn || !IsWindow(btn)) return 0;
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
    TabBarState* s = bar_state(bar);
    if (!s) return 0;
    int w = 0;
    for (int i = 0; i < s->count; i++) w += tab_width(tab_button(bar, i));

    TEXTMETRICW tm = {0};
    HDC hdc = GetDC(bar);
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

static void tab_bar_layout(HWND bar, const TabBarState* s) {
    RECT client;
    GetClientRect(bar, &client);
    int x = 0;
    for (int i = 0; i < s->count; i++) {
        HWND btn = tab_button(bar, i);
        int w = tab_width(btn);
        SetWindowPos(btn, NULL, x, 0, w, client.bottom, SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS);
        x += w;
    }
}

/* Syncs each button's check with the selected index; announces the new one. */
static void tab_bar_show_selected(HWND bar, const TabBarState* s) {
    for (int i = 0; i < s->count; i++) {
        HWND btn = tab_button(bar, i);
        int on = i == s->selected;
        if ((SendMessageW(btn, BM_GETCHECK, 0, 0) == BST_CHECKED) == on) continue;
        SendMessageW(btn, BM_SETCHECK, on ? BST_CHECKED : BST_UNCHECKED, 0);
        if (on) NotifyWinEvent(EVENT_OBJECT_SELECTION, btn, OBJID_CLIENT, CHILDID_SELF);
    }
}

/* Read as a selected page tab, not a checked check box. */
static long tab_state(HWND btn) {
    TabBarState* s = bar_state(GetParent(btn));
    long state = STATE_SYSTEM_SELECTABLE | STATE_SYSTEM_FOCUSABLE;
    if (s && GetDlgCtrlID(btn) - TABBAR_BASE_ID == s->selected) state |= STATE_SYSTEM_SELECTED;
    if (GetFocus() == btn) state |= STATE_SYSTEM_FOCUSED;
    if (!IsWindowEnabled(btn)) state |= STATE_SYSTEM_UNAVAILABLE;
    if (!IsWindowVisible(btn)) state |= STATE_SYSTEM_INVISIBLE;
    return state;
}

static LRESULT CALLBACK tab_bar_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                     UINT_PTR id, DWORD_PTR data) {
    TabBarState* s = (TabBarState*)data;
    switch (msg) {
    case WM_COMMAND:
        if (HIWORD(wParam) == BN_CLICKED) {
            int idx = (int)LOWORD(wParam) - TABBAR_BASE_ID;
            if (idx >= 0 && idx < s->count) {
                if (idx != s->selected) {
                    s->selected = idx;
                    tab_bar_show_selected(hwnd, s);
                    /* The handler may destroy this bar and free `s`; nothing touches it after. */
                    ng_invoke_tab_bar_selected(s->id, idx);
                }
                return 0;
            }
        }
        break;
    case WM_SIZE:
        tab_bar_layout(hwnd, s);
        break;
    case WM_ERASEBKGND: {
        RECT client;
        GetClientRect(hwnd, &client);
        FillRect((HDC)wParam, &client, GetSysColorBrush(COLOR_BTNFACE));
        return 1;
    }
    case WM_NCDESTROY:
        ng_windows_remove_subclass(hwnd, tab_bar_proc, id);
        free(s);
        break;
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
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

    TabBarState* s = (TabBarState*)calloc(1, sizeof(TabBarState));
    if (!s) {
        DestroyWindow(container);
        return NULL;
    }
    s->id = id;
    s->selected = -1;
    if (!ng_windows_set_subclass(container, tab_bar_proc, TABBAR_SUBCLASS_ID, (DWORD_PTR)s)) {
        free(s);
        DestroyWindow(container);
        return NULL;
    }
    ng_windows_set_role(container, ROLE_SYSTEM_PAGETABLIST);
    return (NGHandle)container;
}

int ng_windows_tab_bar_add_tab(NGHandle tab_bar, const char* title) {
    if (!tab_bar || !title) return NG_ERROR_INVALID_PARAMETER;
    HWND bar = (HWND)tab_bar;
    if (!IsWindow(bar)) return NG_ERROR_INVALID_HANDLE;
    TabBarState* s = bar_state(bar);
    if (!s) return NG_ERROR_INVALID_HANDLE;

    char* shown = ng_windows_escape_mnemonics(title);
    if (!shown) return NG_ERROR_INVALID_PARAMETER;
    HWND btn = ng_windows_create_control(
        0, L"BUTTON", shown, WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_CHECKBOX | BS_PUSHLIKE,
        80, 24, bar, TABBAR_BASE_ID + s->count);
    free(shown);
    if (!btn) return NG_ERROR_CREATION_FAILED;
    SendMessageW(btn, WM_SETFONT, (WPARAM)ng_windows_ui_font(bar), FALSE);
    ng_windows_set_role(btn, ROLE_SYSTEM_PAGETAB);
    ng_windows_serve_state(btn, tab_state);

    s->count++;
    if (s->selected < 0) s->selected = 0;
    tab_bar_show_selected(bar, s);
    tab_bar_layout(bar, s);
    ng_windows_relayout(bar);
    return NG_SUCCESS;
}

int ng_windows_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    if (!tab_bar) return NG_ERROR_INVALID_HANDLE;
    HWND bar = (HWND)tab_bar;
    if (!IsWindow(bar)) return NG_ERROR_INVALID_HANDLE;
    TabBarState* s = bar_state(bar);
    if (!s) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= s->count) return NG_ERROR_INVALID_PARAMETER;

    DestroyWindow(tab_button(bar, index));
    for (int i = index + 1; i < s->count; i++) {
        SetWindowLongPtrW(tab_button(bar, i), GWLP_ID, TABBAR_BASE_ID + i - 1);
    }
    s->count--;
    if (s->selected > index || s->selected >= s->count) s->selected--;
    if (s->selected < 0 && s->count > 0) s->selected = 0;
    tab_bar_show_selected(bar, s);
    tab_bar_layout(bar, s);
    ng_windows_relayout(bar);
    return NG_SUCCESS;
}

int ng_windows_tab_bar_set_selected(NGHandle tab_bar, int index) {
    if (!tab_bar) return NG_ERROR_INVALID_HANDLE;
    HWND bar = (HWND)tab_bar;
    if (!IsWindow(bar)) return NG_ERROR_INVALID_HANDLE;
    TabBarState* s = bar_state(bar);
    if (!s) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= s->count) return NG_ERROR_INVALID_PARAMETER;
    s->selected = index;
    tab_bar_show_selected(bar, s);
    return NG_SUCCESS;
}

int ng_windows_tab_bar_get_selected(NGHandle tab_bar) {
    TabBarState* s = tab_bar ? bar_state((HWND)tab_bar) : NULL;
    return s ? s->selected : -1;
}

void ng_windows_tab_bar_invalidate(NGHandle tab_bar) {
    if (!tab_bar) return;
    InvalidateRect((HWND)tab_bar, NULL, FALSE);
}
