#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include <windows.h>

NGHandle ng_windows_create_label(const char* text) {
    if (!text) return NULL;

    HWND label = ng_windows_create_control(
        0, L"STATIC", text, WS_CHILD | SS_LEFT | WS_VISIBLE,
        200, 20, ng_windows_detached_parent(), 0);

    if (label) {
        SetPropA(label, BOX_LABEL_PROP, (HANDLE)1);
        HDC hdc = GetDC(label);
        if (hdc) {
            HFONT hfont = (HFONT)SendMessageW(label, WM_GETFONT, 0, 0);
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

int ng_windows_label_set_text(NGHandle label, const char* text) {
    if (!label) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    int result = ng_windows_set_window_text((HWND)label, text);
    if (result == NG_SUCCESS) ng_windows_relayout((HWND)label);
    return result;
}

void ng_windows_label_invalidate(NGHandle label) {
    if (!label) return;
    InvalidateRect((HWND)label, NULL, FALSE);
}
