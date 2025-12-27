#include "common.h"
#include "../elements.h"
#include "../../common/errors.h"
#include <windows.h>

NGHandle ng_windows_create_label(const char* text) {
    if (!text) return NULL;

    HWND temp_parent = GetDesktopWindow();

    HWND label = CreateWindowExA(
        0,
        "STATIC",
        text,
        WS_CHILD | SS_LEFT | WS_VISIBLE,
        0, 0, 200, 20,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (label) {
        HDC hdc = GetDC(label);
        if (hdc) {
            HFONT hfont = (HFONT)SendMessageA(label, WM_GETFONT, 0, 0);
            if (!hfont) {
                hfont = (HFONT)GetStockObject(DEFAULT_GUI_FONT);
            }
            HFONT old_font = (HFONT)SelectObject(hdc, hfont);

            int text_width, text_height;
            calculate_text_size(hdc, text, &text_width, &text_height);

            SetWindowPos(label, NULL, 0, 0,
                       200,
                       text_height + LABEL_PADDING * 2,
                       SWP_NOMOVE | SWP_NOZORDER);

            SelectObject(hdc, old_font);
            ReleaseDC(label, hdc);
        }
    }

    return (NGHandle)label;
}

void ng_windows_label_invalidate(NGHandle label) {
    if (!label) return;
    InvalidateRect((HWND)label, NULL, FALSE);
}

