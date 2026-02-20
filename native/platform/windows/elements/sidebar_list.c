#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <stdlib.h>
#include <string.h>

#define SIDEBAR_ID_PROP "AureaSidebarId"
#define SIDEBAR_OLD_PROC_PROP "AureaSidebarOldProc"
#define SIDEBAR_NEXT_ITEM_PROP "AureaSidebarNextItem"
#define SIDEBAR_SELECTED_PROP "AureaSidebarSelected"
#define SIDEBAR_CURRENT_Y_PROP "AureaSidebarCurrentY"
#define SIDEBAR_BASE_ID 9500
#define ROW_HEIGHT 18
#define SECTION_PADDING 4
#define INDENT_STEP 10
#define LEFT_MARGIN 6

static LRESULT CALLBACK SidebarProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    if (msg == WM_COMMAND) {
        unsigned int id = (unsigned int)GetPropA(hwnd, SIDEBAR_ID_PROP);
        if (id) {
            int idx = (int)(LOWORD(wParam) - SIDEBAR_BASE_ID);
            if (idx >= 0) {
                SetPropA(hwnd, SIDEBAR_SELECTED_PROP, (HANDLE)(INT_PTR)idx);
                ng_invoke_sidebar_list_selected(id, idx);
                return 0;
            }
        }
    } else if (msg == WM_NCDESTROY) {
        RemovePropA(hwnd, SIDEBAR_ID_PROP);
        RemovePropA(hwnd, SIDEBAR_OLD_PROC_PROP);
        RemovePropA(hwnd, SIDEBAR_NEXT_ITEM_PROP);
        RemovePropA(hwnd, SIDEBAR_SELECTED_PROP);
        RemovePropA(hwnd, SIDEBAR_CURRENT_Y_PROP);
    }

    WNDPROC old_proc = (WNDPROC)GetPropA(hwnd, SIDEBAR_OLD_PROC_PROP);
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
        WS_CHILD | SS_LEFT | WS_VISIBLE | WS_VSCROLL,
        0, 0, 200, 400,
        GetDesktopWindow(),
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (container) {
        SetPropA(container, SIDEBAR_ID_PROP, (HANDLE)(UINT_PTR)id);
        SetPropA(container, SIDEBAR_NEXT_ITEM_PROP, (HANDLE)0);
        SetPropA(container, SIDEBAR_SELECTED_PROP, (HANDLE)-1);
        SetPropA(container, SIDEBAR_CURRENT_Y_PROP, (HANDLE)0);
        SetClassLongPtrA(container, GCLP_HBRBACKGROUND, (LONG_PTR)GetStockObject(NULL_BRUSH));
        WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(container, GWLP_WNDPROC, (LONG_PTR)SidebarProc);
        if (old_proc) {
            SetPropA(container, SIDEBAR_OLD_PROC_PROP, (HANDLE)old_proc);
        }
    }
    return (NGHandle)container;
}

static int get_current_y(HWND bar) {
    return (int)(INT_PTR)GetPropA(bar, SIDEBAR_CURRENT_Y_PROP);
}

static void set_current_y(HWND bar, int y) {
    SetPropA(bar, SIDEBAR_CURRENT_Y_PROP, (HANDLE)(INT_PTR)y);
}

int ng_windows_sidebar_list_add_section(NGHandle sidebar, const char* title) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;

    HWND bar = (HWND)sidebar;
    int y = get_current_y(bar);

    HWND label = CreateWindowExA(
        0,
        "STATIC",
        title,
        WS_CHILD | SS_LEFT | WS_VISIBLE,
        LEFT_MARGIN, y, 200, ROW_HEIGHT,
        bar,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    if (!label) return NG_ERROR_CREATION_FAILED;
    set_current_y(bar, y + ROW_HEIGHT + SECTION_PADDING);
    return NG_SUCCESS;
}

int ng_windows_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;

    HWND bar = (HWND)sidebar;
    int item_idx = (int)(INT_PTR)GetPropA(bar, SIDEBAR_NEXT_ITEM_PROP);
    SetPropA(bar, SIDEBAR_NEXT_ITEM_PROP, (HANDLE)(INT_PTR)(item_idx + 1));

    int y = get_current_y(bar);
    UINT btn_id = SIDEBAR_BASE_ID + item_idx;
    int x = LEFT_MARGIN + indent * INDENT_STEP;

    HWND btn = CreateWindowExA(
        0,
        "BUTTON",
        title,
        WS_CHILD | BS_PUSHBUTTON | BS_FLAT | WS_VISIBLE,
        x, y, 200 - x, ROW_HEIGHT - 2,
        bar,
        (HMENU)(UINT_PTR)btn_id,
        GetModuleHandleA(NULL),
        NULL
    );
    if (!btn) return NG_ERROR_CREATION_FAILED;
    set_current_y(bar, y + ROW_HEIGHT);
    return NG_SUCCESS;
}

int ng_windows_sidebar_list_set_selected(NGHandle sidebar, int index) {
    if (!sidebar) return NG_ERROR_INVALID_HANDLE;
    SetPropA((HWND)sidebar, SIDEBAR_SELECTED_PROP, (HANDLE)(INT_PTR)index);
    return NG_SUCCESS;
}

int ng_windows_sidebar_list_get_selected(NGHandle sidebar) {
    if (!sidebar) return -1;
    return (int)(INT_PTR)GetPropA((HWND)sidebar, SIDEBAR_SELECTED_PROP);
}

int ng_windows_sidebar_list_clear(NGHandle sidebar) {
    if (!sidebar) return NG_ERROR_INVALID_HANDLE;
    HWND bar = (HWND)sidebar;
    HWND child = GetWindow(bar, GW_CHILD);
    while (child) {
        HWND next = GetWindow(child, GW_HWNDNEXT);
        DestroyWindow(child);
        child = next;
    }
    SetPropA(bar, SIDEBAR_NEXT_ITEM_PROP, (HANDLE)0);
    SetPropA(bar, SIDEBAR_SELECTED_PROP, (HANDLE)-1);
    SetPropA(bar, SIDEBAR_CURRENT_Y_PROP, (HANDLE)0);
    return NG_SUCCESS;
}

void ng_windows_sidebar_list_invalidate(NGHandle sidebar) {
    if (!sidebar) return;
    InvalidateRect((HWND)sidebar, NULL, FALSE);
}
