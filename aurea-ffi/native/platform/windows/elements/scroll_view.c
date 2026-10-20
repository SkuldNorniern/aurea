#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>

#define SCROLL_LINE 16

static LRESULT CALLBACK ScrollViewProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);

static HWND scroll_content(HWND scroll) {
    return GetWindow(scroll, GW_CHILD);
}

int ng_windows_is_scroll_view(HWND hwnd) {
    return hwnd && GetPropA(hwnd, SCROLL_VIEW_OLD_PROC_PROP) != NULL;
}

void ng_windows_scroll_view_layout(HWND scroll) {
    if (!ng_windows_is_scroll_view(scroll)) return;
    HWND content = scroll_content(scroll);
    RECT client;
    GetClientRect(scroll, &client);
    int width = client.right;
    int page = client.bottom;
    int height = content ? ng_windows_measure_height(content, width) : 0;
    if (height < page) height = page;

    SCROLLINFO si = {sizeof(si), SIF_RANGE | SIF_PAGE | SIF_DISABLENOSCROLL};
    si.nMax = height > 0 ? height - 1 : 0;
    si.nPage = (UINT)(page > 0 ? page : 0);
    SetScrollInfo(scroll, SB_VERT, &si, TRUE);
    si.fMask = SIF_POS;
    GetScrollInfo(scroll, SB_VERT, &si);

    if (content) {
        SetWindowPos(content, NULL, 0, -si.nPos, width, height, SWP_NOZORDER | SWP_NOACTIVATE);
    }
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
        SCROLLINFO si = {sizeof(si), SIF_POS};
        GetScrollInfo(hwnd, SB_VERT, &si);
        UINT lines = 3;
        SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &lines, 0);
        int delta = GET_WHEEL_DELTA_WPARAM(wParam);
        scroll_to(hwnd, si.nPos - delta * (int)lines * ng_windows_px(hwnd, SCROLL_LINE) / WHEEL_DELTA);
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
    if (msg == WM_NCDESTROY) RemovePropA(hwnd, SCROLL_VIEW_OLD_PROC_PROP);
    return old_proc ? CallWindowProcA(old_proc, hwnd, msg, wParam, lParam)
                    : DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_scroll_view(void) {
    HWND scroll = CreateWindowExA(0, "STATIC", NULL,
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
