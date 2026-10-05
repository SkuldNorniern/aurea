#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>
#include <string.h>

static const char* BOX_OLD_PROC_PROP = "AureaBoxOldProc";

static LRESULT CALLBACK BoxProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    if (msg == WM_COMMAND) {
        HWND parent = GetParent(hwnd);
        while (parent && parent != GetDesktopWindow()) {
            char class_name[256];
            GetClassNameA(parent, class_name, sizeof(class_name));
            if (_stricmp(class_name, "NativeGuiWindow") == 0) {
                SendMessageA(parent, msg, wParam, lParam);
                break;
            }
            parent = GetParent(parent);
        }
    } else if (msg == WM_ERASEBKGND) {
        /* Paint the gaps between children in the dialog colour, around the
           children so a canvas filling the box does not flicker. Left
           transparent, the gaps kept whatever the previous content drew. */
        HDC hdc = (HDC)wParam;
        int saved = SaveDC(hdc);
        for (HWND child = GetWindow(hwnd, GW_CHILD); child;
             child = GetWindow(child, GW_HWNDNEXT)) {
            if (!(GetWindowLongPtrA(child, GWL_STYLE) & WS_VISIBLE)) continue;
            RECT r;
            GetWindowRect(child, &r);
            MapWindowPoints(NULL, hwnd, (POINT*)&r, 2);
            ExcludeClipRect(hdc, r.left, r.top, r.right, r.bottom);
        }
        RECT client;
        GetClientRect(hwnd, &client);
        FillRect(hdc, &client, GetSysColorBrush(COLOR_BTNFACE));
        RestoreDC(hdc, saved);
        return 1;
    } else if (msg == WM_NCDESTROY) {
        RemovePropA(hwnd, BOX_OLD_PROC_PROP);
    }
    
    WNDPROC old_proc = (WNDPROC)GetPropA(hwnd, BOX_OLD_PROC_PROP);
    if (old_proc) {
        return CallWindowProcA(old_proc, hwnd, msg, wParam, lParam);
    }
    return DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_box(int is_vertical) {
    HWND temp_parent = ng_windows_detached_parent();

    HWND container = CreateWindowExA(
        0,
        "STATIC",
        NULL,
        WS_CHILD | SS_LEFT | WS_VISIBLE,
        0, 0, 100, 100,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (container) {
        SetPropA(container, BOX_ORIENTATION_PROP,
                 (HANDLE)(INT_PTR)(is_vertical ? BOX_VERTICAL : BOX_HORIZONTAL));
        SetClassLongPtrA(container, GCLP_HBRBACKGROUND, (LONG_PTR)GetStockObject(NULL_BRUSH));
        WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(container, GWLP_WNDPROC, (LONG_PTR)BoxProc);
        if (old_proc) {
            SetPropA(container, BOX_OLD_PROC_PROP, (HANDLE)old_proc);
        }
    }

    return (NGHandle)container;
}

int ng_windows_box_add(NGHandle box, NGHandle element) {
    return ng_windows_box_add_weighted(box, element, 0.0f);
}

int ng_windows_box_add_weighted(NGHandle box, NGHandle element, float weight) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;

    HWND box_hwnd = (HWND)box;
    HWND element_hwnd = (HWND)element;

    static INT_PTR next_index = 1;
    SetParent(element_hwnd, box_hwnd);
    SetPropA(element_hwnd, BOX_INDEX_PROP, (HANDLE)next_index++);
    RECT added;
    GetWindowRect(element_hwnd, &added);
    SetPropA(element_hwnd, BOX_NATURAL_PROP,
             (HANDLE)(((INT_PTR)(added.right - added.left) & 0xFFFF) << 16 |
                      ((INT_PTR)(added.bottom - added.top) & 0xFFFF)));
    if (weight > 0.0f) {
        SetPropA(element_hwnd, BOX_WEIGHT_PROP, (HANDLE)(INT_PTR)(weight * 1000.0f + 1.0f));
    } else {
        RemovePropA(element_hwnd, BOX_WEIGHT_PROP);
    }

    LONG_PTR style = GetWindowLongPtrA(element_hwnd, GWL_STYLE);
    SetWindowLongPtrA(element_hwnd, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);

    /* Without a font a control draws in the old bold System font, unlike
       every native dialog. Boxes and canvases have no text to draw. */
    if (!is_box(element_hwnd) && !SendMessageA(element_hwnd, WM_GETFONT, 0, 0)) {
        SendMessageA(element_hwnd, WM_SETFONT, (WPARAM)ng_windows_ui_font(), FALSE);
    }

    char class_name[256];
    GetClassNameA(element_hwnd, class_name, sizeof(class_name));
    if (_stricmp(class_name, "BUTTON") == 0) {
        InvalidateRect(element_hwnd, NULL, TRUE);
        UpdateWindow(element_hwnd);
    }

    ShowWindow(element_hwnd, SW_SHOW);
    layout_box_children(box_hwnd);

    HWND box_parent = GetParent(box_hwnd);
    if (box_parent) {
        char parent_class[256];
        GetClassNameA(box_parent, parent_class, sizeof(parent_class));
        BOOL is_window_parent = (_stricmp(parent_class, "NativeGuiWindow") == 0);

        if (is_window_parent) {
            /* The client area already leaves the menu bar out, as it does
               for the window's resize handler. */
            RECT parent_rect;
            GetClientRect(box_parent, &parent_rect);

            int target_width = parent_rect.right - parent_rect.left;
            int target_height = parent_rect.bottom - parent_rect.top;
            
            SetWindowPos(box_hwnd, NULL, 0, 0,
                        target_width, target_height,
                        SWP_NOMOVE | SWP_NOZORDER);

            layout_box_children(box_hwnd);
        }
        else {
            /* Lay out again from the top, so the parents make room. */
            HWND top = box_hwnd;
            while (GetParent(top) && is_box(GetParent(top))) top = GetParent(top);
            if (top != box_hwnd) layout_box_children(top);
        }
    }

    return NG_SUCCESS;
}

void ng_windows_box_invalidate(NGHandle box) {
    if (!box) return;
    InvalidateRect((HWND)box, NULL, FALSE);
}

