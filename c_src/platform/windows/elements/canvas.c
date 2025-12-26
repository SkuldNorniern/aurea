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

