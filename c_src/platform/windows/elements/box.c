#include "common.h"
#include "../elements.h"
#include "../../../common/errors.h"
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
    HWND temp_parent = GetDesktopWindow();

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
        SetPropA(container, BOX_ORIENTATION_PROP, (HANDLE)(INT_PTR)is_vertical);
        SetClassLongPtrA(container, GCLP_HBRBACKGROUND, (LONG_PTR)GetStockObject(NULL_BRUSH));
        WNDPROC old_proc = (WNDPROC)SetWindowLongPtrA(container, GWLP_WNDPROC, (LONG_PTR)BoxProc);
        if (old_proc) {
            SetPropA(container, BOX_OLD_PROC_PROP, (HANDLE)old_proc);
        }
    }

    return (NGHandle)container;
}

int ng_windows_box_add(NGHandle box, NGHandle element) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;

    HWND box_hwnd = (HWND)box;
    HWND element_hwnd = (HWND)element;

    SetParent(element_hwnd, box_hwnd);

    LONG_PTR style = GetWindowLongPtrA(element_hwnd, GWL_STYLE);
    SetWindowLongPtrA(element_hwnd, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);

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
            RECT parent_rect;
            GetClientRect(box_parent, &parent_rect);

            HMENU menu = GetMenu(box_parent);
            int menu_height = 0;
            if (menu) {
                RECT menu_rect;
                if (GetMenuItemRect(box_parent, menu, 0, &menu_rect)) {
                    POINT pt = {menu_rect.left, menu_rect.top};
                    ScreenToClient(box_parent, &pt);
                    menu_height = menu_rect.bottom - menu_rect.top + pt.y;
                } else {
                    menu_height = GetSystemMetrics(SM_CYMENU);
                }
            }

            int target_width = parent_rect.right - parent_rect.left;
            int target_height = parent_rect.bottom - parent_rect.top - menu_height;
            
            SetWindowPos(box_hwnd, NULL, 0, 0,
                        target_width, target_height,
                        SWP_NOMOVE | SWP_NOZORDER);

            layout_box_children(box_hwnd);
        }
        else if (!is_window_parent) {
            int max_x = PADDING;
            int max_y = PADDING;

            HWND child = GetWindow(box_hwnd, GW_CHILD);
            while (child) {
                if (IsWindowVisible(child)) {
                    RECT child_rect;
                    GetWindowRect(child, &child_rect);
                    POINT pt = {child_rect.left, child_rect.top};
                    ScreenToClient(box_hwnd, &pt);

                    int child_right = pt.x + (child_rect.right - child_rect.left);
                    int child_bottom = pt.y + (child_rect.bottom - child_rect.top);

                    if (child_right > max_x) max_x = child_right;
                    if (child_bottom > max_y) max_y = child_bottom;
                }
                child = GetWindow(child, GW_HWNDNEXT);
            }

            if (max_x > PADDING || max_y > PADDING) {
                SetWindowPos(box_hwnd, NULL, 0, 0, max_x + PADDING, max_y + PADDING,
                            SWP_NOMOVE | SWP_NOZORDER);
            }
        }
    }

    return NG_SUCCESS;
}

void ng_windows_box_invalidate(NGHandle box) {
    if (!box) return;
    InvalidateRect((HWND)box, NULL, FALSE);
}

