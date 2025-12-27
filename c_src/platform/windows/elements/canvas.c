#include "common.h"
#include "../elements.h"
#include "../../common/errors.h"
#include <windows.h>

NGHandle ng_windows_create_canvas(int width, int height) {
    HWND temp_parent = GetDesktopWindow();

    HWND hwnd = CreateWindowExA(
        0,
        "STATIC",
        NULL,
        WS_CHILD,
        0, 0, width, height,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    return (NGHandle)hwnd;
}

void ng_windows_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    InvalidateRect((HWND)canvas, NULL, FALSE);
}

void ng_windows_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    if (!canvas) return;
    RECT rect;
    rect.left = (LONG)x;
    rect.top = (LONG)y;
    rect.right = (LONG)(x + width);
    rect.bottom = (LONG)(y + height);
    InvalidateRect((HWND)canvas, &rect, FALSE);
}

