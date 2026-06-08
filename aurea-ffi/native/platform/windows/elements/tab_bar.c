#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <stdlib.h>
#include <string.h>

#define TABBAR_ID_PROP "AureaTabBarId"
#define TABBAR_OLD_PROC_PROP "AureaTabBarOldProc"
#define TABBAR_BASE_ID 9000

static LRESULT CALLBACK TabBarProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    if (msg == WM_COMMAND) {
        unsigned int id = (unsigned int)GetPropA(hwnd, TABBAR_ID_PROP);
        if (id) {
            int idx = (int)(LOWORD(wParam) - TABBAR_BASE_ID);
            if (idx >= 0) {
                ng_invoke_tab_bar_selected(id, idx);
                return 0;
            }
        }
    } else if (msg == WM_NCDESTROY) {
        RemovePropA(hwnd, TABBAR_ID_PROP);
        RemovePropA(hwnd, TABBAR_OLD_PROC_PROP);
    }

    WNDPROC old_proc = (WNDPROC)GetPropA(hwnd, TABBAR_OLD_PROC_PROP);
    if (old_proc) {
        return CallWindowProcA(old_proc, hwnd, msg, wParam, lParam);
    }
    return DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_tab_bar(unsigned int id) {
    HWND temp_parent = GetDesktopWindow();
    HWND container = CreateWindowExA(
        0,
        "STATIC",
        NULL,
        WS_CHILD | SS_LEFT | WS_VISIBLE,
        0, 0, 100, 28,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (container) {
        SetPropA(container, TABBAR_ID_PROP, (HANDLE)(UINT_PTR)id);
        SetClassLongPtrA(container, GCLP_HBRBACKGROUND, (LONG_PTR)GetStockObject(NULL_BRUSH));
        WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(container, GWLP_WNDPROC, (LONG_PTR)TabBarProc);
        if (old_proc) {
            SetPropA(container, TABBAR_OLD_PROC_PROP, (HANDLE)old_proc);
        }
    }
    return (NGHandle)container;
}

int ng_windows_tab_bar_add_tab(NGHandle tab_bar, const char* title) {
    if (!tab_bar || !title) return NG_ERROR_INVALID_PARAMETER;

    HWND bar = (HWND)tab_bar;
    int count = 0;
    HWND child = GetWindow(bar, GW_CHILD);
    while (child) {
        count++;
        child = GetWindow(child, GW_HWNDNEXT);
    }

    UINT btn_id = TABBAR_BASE_ID + count;
    HWND btn = CreateWindowExA(
        0,
        "BUTTON",
        title,
        WS_CHILD | BS_PUSHBUTTON | WS_VISIBLE,
        0, 0, 80, 24,
        bar,
        (HMENU)(UINT_PTR)btn_id,
        GetModuleHandleA(NULL),
        NULL
    );
    if (!btn) return NG_ERROR_CREATION_FAILED;
    return NG_SUCCESS;
}

int ng_windows_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    (void)tab_bar;
    (void)index;
    return NG_SUCCESS;
}

int ng_windows_tab_bar_set_selected(NGHandle tab_bar, int index) {
    (void)tab_bar;
    (void)index;
    return NG_SUCCESS;
}

int ng_windows_tab_bar_get_selected(NGHandle tab_bar) {
    if (!tab_bar) return -1;
    (void)tab_bar;
    return 0;
}

void ng_windows_tab_bar_invalidate(NGHandle tab_bar) {
    if (!tab_bar) return;
    InvalidateRect((HWND)tab_bar, NULL, FALSE);
}
