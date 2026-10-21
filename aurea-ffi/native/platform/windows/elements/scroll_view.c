#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include "common/platform_api.h"
#include <windows.h>

#define SCROLL_LINE 16
#define SCROLL_WHEEL_REST_PROP "AureaScrollWheelRest"
#define SCROLL_ALWAYS_PROP "AureaScrollAlways"
#define SCROLL_LAYOUT_PROP "AureaScrollLayout"

static LRESULT CALLBACK ScrollViewProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);

static HWND scroll_content(HWND scroll) {
    return GetWindow(scroll, GW_CHILD);
}

int ng_windows_is_scroll_view(HWND hwnd) {
    return hwnd && GetPropA(hwnd, SCROLL_VIEW_OLD_PROC_PROP) != NULL;
}

void ng_windows_scroll_view_layout(HWND scroll) {
    if (!ng_windows_is_scroll_view(scroll) || GetPropA(scroll, SCROLL_LAYOUT_PROP)) return;
    SetPropA(scroll, SCROLL_LAYOUT_PROP, (HANDLE)1);
    HWND content = scroll_content(scroll);
    UINT keep = GetPropA(scroll, SCROLL_ALWAYS_PROP) ? SIF_DISABLENOSCROLL : 0;
    RECT client;
    int height = 0;
    /* Showing or hiding the bar changes the width, and so the content's height. */
    for (int pass = 0; pass < 3; pass++) {
        GetClientRect(scroll, &client);
        height = content ? ng_windows_measure_height(content, client.right) : 0;
        if (height < client.bottom) height = client.bottom;
        SCROLLINFO si = {sizeof(si), SIF_RANGE | SIF_PAGE | keep};
        si.nMax = height > 0 ? height - 1 : 0;
        si.nPage = (UINT)(client.bottom > 0 ? client.bottom : 0);
        SetScrollInfo(scroll, SB_VERT, &si, TRUE);
        RECT now;
        GetClientRect(scroll, &now);
        if (now.right == client.right && now.bottom == client.bottom) break;
    }
    SCROLLINFO si = {sizeof(si), SIF_POS};
    GetScrollInfo(scroll, SB_VERT, &si);

    if (content) {
        SetWindowPos(content, NULL, 0, -si.nPos, client.right, height, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    RemovePropA(scroll, SCROLL_LAYOUT_PROP);
}

int ng_windows_scroll_view_set_scrollbar(NGHandle scroll_handle, int policy) {
    HWND scroll = (HWND)scroll_handle;
    if (!ng_windows_is_scroll_view(scroll)) return NG_ERROR_INVALID_HANDLE;
    if (policy == NG_SCROLLBAR_ALWAYS) {
        SetPropA(scroll, SCROLL_ALWAYS_PROP, (HANDLE)1);
        ShowScrollBar(scroll, SB_VERT, TRUE);
    } else if (policy == NG_SCROLLBAR_AUTO) {
        RemovePropA(scroll, SCROLL_ALWAYS_PROP);
        ShowScrollBar(scroll, SB_VERT, FALSE);
    } else {
        return NG_ERROR_INVALID_PARAMETER;
    }
    ng_windows_scroll_view_layout(scroll);
    return NG_SUCCESS;
}

static void scroll_to(HWND scroll, int pos) {
    SCROLLINFO si = {sizeof(si), SIF_POS};
    si.nPos = pos;
    SetScrollInfo(scroll, SB_VERT, &si, TRUE);
    GetScrollInfo(scroll, SB_VERT, &si);
    HWND content = scroll_content(scroll);
    if (content) {
        SetWindowPos(content, NULL, 0, -si.nPos, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

static void scroll_by(HWND scroll, int code) {
    SCROLLINFO si = {sizeof(si), SIF_ALL};
    GetScrollInfo(scroll, SB_VERT, &si);
    int line = ng_windows_px(scroll, SCROLL_LINE);
    int pos = si.nPos;
    switch (code) {
        case SB_LINEUP: pos -= line; break;
        case SB_LINEDOWN: pos += line; break;
        case SB_PAGEUP: pos -= (int)si.nPage; break;
        case SB_PAGEDOWN: pos += (int)si.nPage; break;
        case SB_TOP: pos = si.nMin; break;
        case SB_BOTTOM: pos = si.nMax; break;
        case SB_THUMBTRACK:
        case SB_THUMBPOSITION: pos = si.nTrackPos; break;
        default: return;
    }
    scroll_to(scroll, pos);
}

int ng_windows_wheel_step(int delta, unsigned int lines, int line_px, int page, int* rest) {
    if (lines == 0 || delta == 0 || page <= 0) {
        *rest = 0;
        return 0;
    }
    long long unit = lines == WHEEL_PAGESCROLL || (long long)lines * line_px >= page
                         ? page
                         : (long long)lines * line_px;
    if ((*rest < 0) != (delta < 0)) *rest = 0;
    long long total = *rest + (long long)delta * unit;
    long long step = total / WHEEL_DELTA;
    *rest = (int)(total - step * WHEEL_DELTA);
    return (int)-step;
}

static void reveal(HWND scroll, HWND focus) {
    SCROLLINFO si = {sizeof(si), SIF_POS};
    GetScrollInfo(scroll, SB_VERT, &si);
    RECT view, target;
    GetClientRect(scroll, &view);
    GetWindowRect(focus, &target);
    MapWindowPoints(NULL, scroll, (POINT*)&target, 2);
    if (target.top < view.top) {
        scroll_to(scroll, si.nPos + target.top - view.top);
    } else if (target.bottom > view.bottom) {
        int by = target.bottom - view.bottom;
        int room = target.top - view.top;
        scroll_to(scroll, si.nPos + (by < room ? by : room));
    }
}

static void CALLBACK focus_changed(HWINEVENTHOOK hook, DWORD event, HWND hwnd, LONG object,
                                   LONG child, DWORD thread, DWORD time) {
    (void)hook; (void)event; (void)object; (void)child; (void)thread; (void)time;
    if (!hwnd || !IsWindow(hwnd)) return;
    for (HWND at = GetParent(hwnd); at; at = GetParent(at)) {
        if (ng_windows_is_scroll_view(at)) reveal(at, hwnd);
    }
}

static LRESULT CALLBACK ScrollViewProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    if (msg == WM_VSCROLL && lParam == 0) {
        scroll_by(hwnd, LOWORD(wParam));
        return 0;
    }
    if (ng_windows_is_notification(msg)) {
        ng_windows_forward_to_window(hwnd, msg, wParam, lParam);
    }
    if (msg == WM_SIZE) {
        ng_windows_scroll_view_layout(hwnd);
    } else if (msg == WM_MOUSEWHEEL) {
        SCROLLINFO si = {sizeof(si), SIF_POS | SIF_PAGE};
        GetScrollInfo(hwnd, SB_VERT, &si);
        UINT lines = 3;
        SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &lines, 0);
        int rest = (int)(INT_PTR)GetPropA(hwnd, SCROLL_WHEEL_REST_PROP);
        int by = ng_windows_wheel_step(GET_WHEEL_DELTA_WPARAM(wParam), lines,
                                       ng_windows_px(hwnd, SCROLL_LINE), (int)si.nPage, &rest);
        SetPropA(hwnd, SCROLL_WHEEL_REST_PROP, (HANDLE)(INT_PTR)rest);
        if (by) scroll_to(hwnd, si.nPos + by);
        return 0;
    } else if (msg == WM_NCHITTEST) {
        LRESULT hit = DefWindowProcA(hwnd, msg, wParam, lParam);
        return hit == HTVSCROLL ? hit : HTCLIENT;
    } else if (msg == WM_ERASEBKGND) {
        RECT client;
        GetClientRect(hwnd, &client);
        FillRect((HDC)wParam, &client, GetSysColorBrush(COLOR_BTNFACE));
        return 1;
    }

    WNDPROC old_proc = (WNDPROC)GetPropA(hwnd, SCROLL_VIEW_OLD_PROC_PROP);
    if (msg == WM_NCDESTROY) {
        RemovePropA(hwnd, SCROLL_VIEW_OLD_PROC_PROP);
        RemovePropA(hwnd, SCROLL_WHEEL_REST_PROP);
        RemovePropA(hwnd, SCROLL_ALWAYS_PROP);
        RemovePropA(hwnd, SCROLL_LAYOUT_PROP);
    }
    return old_proc ? CallWindowProcA(old_proc, hwnd, msg, wParam, lParam)
                    : DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_scroll_view(void) {
    HWND scroll = CreateWindowExA(WS_EX_CONTROLPARENT, "STATIC", NULL,
                                  WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                                  0, 0, 100, 100, ng_windows_detached_parent(), NULL,
                                  GetModuleHandleA(NULL), NULL);
    if (!scroll) return NULL;
    static DWORD hooked_thread;
    if (hooked_thread != GetCurrentThreadId() &&
        SetWinEventHook(EVENT_OBJECT_FOCUS, EVENT_OBJECT_FOCUS, NULL, focus_changed,
                        GetCurrentProcessId(), GetCurrentThreadId(), WINEVENT_OUTOFCONTEXT)) {
        hooked_thread = GetCurrentThreadId();
    }
    WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(scroll, GWLP_WNDPROC, (LONG_PTR)ScrollViewProc);
    SetPropA(scroll, SCROLL_VIEW_OLD_PROC_PROP, (HANDLE)old_proc);
    ng_windows_mark_container(scroll);
    ng_windows_scroll_view_layout(scroll);
    return (NGHandle)scroll;
}

int ng_windows_scroll_view_set_content(NGHandle scroll_handle, NGHandle element) {
    HWND scroll = (HWND)scroll_handle;
    HWND content = (HWND)element;
    if (!ng_windows_is_scroll_view(scroll) || !content || !IsWindow(content)) return NG_ERROR_INVALID_HANDLE;

    HWND old = scroll_content(scroll);
    if (old == content) return NG_SUCCESS;
    SetParent(content, scroll);
    LONG_PTR style = GetWindowLongPtrA(content, GWL_STYLE);
    SetWindowLongPtrA(content, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);
    if (!is_box(content) && !SendMessageW(content, WM_GETFONT, 0, 0)) {
        SendMessageW(content, WM_SETFONT, (WPARAM)ng_windows_ui_font(scroll), FALSE);
    }
    if (old) DestroyWindow(old);

    scroll_to(scroll, 0);
    ng_windows_scroll_view_layout(scroll);
    if (is_box(content)) layout_box_children(content);
    ShowWindow(content, SW_SHOW);
    return NG_SUCCESS;
}
