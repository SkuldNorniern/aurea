#include "common.h"
#include "../window_state.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include "common/platform_api.h"
#include <commctrl.h>
#include <richedit.h>
#include <stdlib.h>
#include <string.h>

static WNDPROC g_parking_proc = NULL;

static LRESULT CALLBACK parking_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    HWND from = NULL;
    if (msg == WM_COMMAND || msg == WM_HSCROLL || msg == WM_VSCROLL) from = (HWND)lParam;
    else if (msg == WM_NOTIFY && lParam) from = ((NMHDR*)lParam)->hwndFrom;
    else if (msg >= WM_CTLCOLORMSGBOX && msg <= WM_CTLCOLORSTATIC) from = (HWND)lParam;
    if (from) {
        HWND parent = GetParent(from);
        if (parent && parent != hwnd) return SendMessageW(parent, msg, wParam, lParam);
    }
    return CallWindowProcA(g_parking_proc, hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_creation_parent(void) {
    return (NGHandle)ng_windows_detached_parent();
}

HWND ng_windows_detached_parent(void) {
    static HWND parking = NULL;
    if (!parking || !IsWindow(parking)) {
        parking = CreateWindowExA(WS_EX_TOOLWINDOW, "STATIC", NULL, WS_POPUP,
                                  0, 0, 0, 0, NULL, NULL, GetModuleHandleA(NULL), NULL);
        if (parking) {
            g_parking_proc = (WNDPROC)SetWindowLongPtrA(parking, GWLP_WNDPROC, (LONG_PTR)parking_proc);
        }
    }
    return parking ? parking : GetDesktopWindow();
}

HBRUSH ng_windows_row_brush(HWND hwnd) {
    static HBRUSH brush;
    static COLORREF made = CLR_INVALID;
    for (HWND at = hwnd; at; at = GetParent(at)) {
        if (!GetPropA(at, ROW_SELECTED_PROP)) {
            if (GetPropA(at, ROW_LIST_PROP)) return GetSysColorBrush(COLOR_WINDOW);
            continue;
        }
        /* A quarter of the highlight over the window colour, so text keeps its own colour. */
        COLORREF hi = GetSysColor(COLOR_HIGHLIGHT), bg = GetSysColor(COLOR_WINDOW);
        COLORREF mix = RGB((GetRValue(hi) + 3 * GetRValue(bg)) / 4, (GetGValue(hi) + 3 * GetGValue(bg)) / 4,
                           (GetBValue(hi) + 3 * GetBValue(bg)) / 4);
        if (mix != made) {
            if (brush) DeleteObject(brush);
            brush = CreateSolidBrush(mix);
            made = mix;
        }
        return brush;
    }
    return NULL;
}

HWND ng_windows_create_control(DWORD ex_style, const wchar_t* cls, const char* text,
                               DWORD style, int width, int height, HWND parent, UINT id) {
    ng_windows_activate_controls_v6();
    wchar_t* wide = ng_windows_utf8_to_wide(text ? text : "");
    if (!wide) return NULL;
    HWND hwnd = CreateWindowExW(ex_style, cls, wide, style, 0, 0, width, height, parent,
                                (HMENU)(UINT_PTR)id, GetModuleHandleW(NULL), NULL);
    free(wide);
    return hwnd;
}

#define CHANGE_ID_PROP "AureaChangeId"

int ng_windows_is_notification(UINT msg) {
    return msg == WM_COMMAND || msg == WM_NOTIFY || msg == WM_HSCROLL || msg == WM_VSCROLL;
}

void ng_windows_forward_to_window(HWND from, UINT msg, WPARAM wParam, LPARAM lParam) {
    HWND root = ng_windows_root_window(GetParent(from));
    if (root) SendMessageW(root, msg, wParam, lParam);
}

int ng_windows_element_set_change_id(NGHandle element, unsigned int id) {
    if (!element || !IsWindow((HWND)element)) return NG_ERROR_INVALID_HANDLE;
    HWND hwnd = (HWND)element;
    SetPropA(hwnd, CHANGE_ID_PROP, (HANDLE)(UINT_PTR)id);
    if (ng_windows_is_rich_edit(hwnd)) {
        LRESULT mask = SendMessageW(hwnd, EM_GETEVENTMASK, 0, 0);
        SendMessageW(hwnd, EM_SETEVENTMASK, 0, mask | ENM_CHANGE);
    }
    return NG_SUCCESS;
}

int ng_windows_control_changed(UINT msg, WPARAM wParam, LPARAM lParam) {
    HWND control = (HWND)lParam;
    if (!control) return 0;
    unsigned int id = (unsigned int)(UINT_PTR)GetPropA(control, CHANGE_ID_PROP);
    if (!id) return 0;
    int changed = 0;
    if (msg == WM_COMMAND && HIWORD(wParam) == EN_CHANGE) {
        if (GetPropA(control, TEXT_QUIET_PROP)) return 1;
        changed = 1;
    } else if (msg == WM_COMMAND) {
        changed = HIWORD(wParam) == BN_CLICKED || HIWORD(wParam) == CBN_SELCHANGE;
    } else if (msg == WM_HSCROLL || msg == WM_VSCROLL) {
        changed = LOWORD(wParam) != TB_ENDTRACK;
    }
    if (changed) ng_invoke_custom_callback(id);
    return changed;
}

static BOOL CALLBACK enable_child(HWND child, LPARAM enabled) {
    EnableWindow(child, (BOOL)enabled);
    return TRUE;
}

int ng_windows_element_set_enabled(NGHandle element, int enabled) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    EnableWindow(hwnd, enabled ? TRUE : FALSE);
    EnumChildWindows(hwnd, enable_child, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

static LRESULT CALLBACK destroy_watch_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                           UINT_PTR subclass, DWORD_PTR id) {
    if (msg == WM_NCDESTROY) {
        ng_windows_remove_subclass(hwnd, destroy_watch_proc, subclass);
        ng_invoke_custom_callback((unsigned int)id);
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

int ng_windows_element_watch_destroy(NGHandle element, unsigned int id) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    return ng_windows_set_subclass(hwnd, destroy_watch_proc, id, id) ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_windows_element_set_visible(NGHandle element, int visible) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    ShowWindow(hwnd, visible ? SW_SHOW : SW_HIDE);
    ng_windows_relayout(hwnd);
    return NG_SUCCESS;
}

int ng_windows_element_set_min_size(NGHandle element, int width, int height) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (width < 0 || height < 0) return NG_ERROR_INVALID_PARAMETER;
    SetPropA(hwnd, MIN_WIDTH_PROP, (HANDLE)(INT_PTR)(width + 1));
    SetPropA(hwnd, MIN_HEIGHT_PROP, (HANDLE)(INT_PTR)(height + 1));
    ng_windows_relayout(hwnd);
    return NG_SUCCESS;
}

/* One tooltip window for the app, showing whichever tool the pointer rests on. */
static HWND ng_windows_tooltips(void) {
    static HWND tips = NULL;
    if (!tips || !IsWindow(tips)) {
        ng_windows_activate_controls_v6();
        ng_windows_init_common_controls(ICC_BAR_CLASSES);
        tips = CreateWindowExW(WS_EX_TOPMOST, TOOLTIPS_CLASSW, NULL, WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
                               CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, NULL, NULL,
                               GetModuleHandleW(NULL), NULL);
        if (tips) SendMessageW(tips, TTM_SETMAXTIPWIDTH, 0, 400);
    }
    return tips;
}

int ng_windows_element_set_tooltip(NGHandle element, const char* text) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    HWND tips = ng_windows_tooltips();
    if (!tips) return NG_ERROR_PLATFORM_SPECIFIC;
    TTTOOLINFOW tool;
    ZeroMemory(&tool, sizeof(tool));
    tool.cbSize = sizeof(tool);
    tool.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
    tool.hwnd = hwnd;
    tool.uId = (UINT_PTR)hwnd;
    SendMessageW(tips, TTM_DELTOOLW, 0, (LPARAM)&tool);
    if (!*text) return NG_SUCCESS;
    wchar_t* wide = ng_windows_utf8_to_wide(text);
    if (!wide) return NG_ERROR_INVALID_PARAMETER;
    tool.lpszText = wide;
    LRESULT added = SendMessageW(tips, TTM_ADDTOOLW, 0, (LPARAM)&tool);
    free(wide);
    return added ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

void ng_windows_unlimit_text(HWND edit) {
    SendMessageW(edit, EM_EXLIMITTEXT, 0, 0x7FFFFFFE);
}

void calculate_text_size(HDC hdc, const char* text, int* width, int* height) {
    LOG_TRACE("calculate_text_size: called with text='%s'", text ? text : "(null)");
    
    if (!hdc || !text || !width || !height) {
        LOG_WARN("calculate_text_size: Invalid parameters");
        return;
    }
    
    SIZE text_size;
    wchar_t* wide = ng_windows_utf8_to_wide(text);

    if (wide && GetTextExtentPoint32W(hdc, wide, (int)wcslen(wide), &text_size)) {
        *width = text_size.cx;
        *height = text_size.cy;
    } else {
        *width = 100;
        *height = 20;
    }
    free(wide);
}

#ifndef NG_FEATURE_WIDGETS
/* Without the widgets the layout is asked about no control, so every answer is no. */
int ng_windows_is_rich_edit(HWND hwnd) {
    (void)hwnd;
    return 0;
}

int ng_windows_is_scroll_view(HWND hwnd) {
    (void)hwnd;
    return 0;
}

int ng_windows_is_split_view(HWND hwnd) {
    (void)hwnd;
    return 0;
}

int ng_windows_tab_bar_measure(HWND bar, int* width, int* height) {
    (void)bar;
    (void)width;
    (void)height;
    return 0;
}

void ng_windows_scroll_view_layout(HWND scroll) {
    (void)scroll;
}
#endif
