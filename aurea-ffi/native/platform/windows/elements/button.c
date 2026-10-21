#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include "../utils.h"
#include <windows.h>
#include <stdlib.h>

NGHandle ng_windows_create_button(const char* title, unsigned int id) {
    if (!title) return NULL;
    char* shown = ng_windows_escape_mnemonics(title);
    if (!shown) return NULL;

    HWND button = ng_windows_create_control(
        0, L"BUTTON", shown, WS_CHILD | BS_PUSHBUTTON | WS_VISIBLE,
        BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT,
        ng_windows_detached_parent(), id + BUTTON_COMMAND_BASE);
    free(shown);

    if (button) {
        HDC hdc = GetDC(button);
        if (hdc) {
            HFONT hfont = (HFONT)SendMessageW(button, WM_GETFONT, 0, 0);
            if (!hfont) {
                hfont = (HFONT)GetStockObject(DEFAULT_GUI_FONT);
            }
            HFONT old_font = (HFONT)SelectObject(hdc, hfont);

            int text_width, text_height;
            calculate_text_size(hdc, title, &text_width, &text_height);

            int button_width = text_width + 32;
            int button_height = text_height + 16;

            if (button_width < BUTTON_MIN_WIDTH) button_width = BUTTON_MIN_WIDTH;
            if (button_height < BUTTON_MIN_HEIGHT) button_height = BUTTON_MIN_HEIGHT;

            SetWindowPos(button, NULL, 0, 0, button_width, button_height,
                       SWP_NOMOVE | SWP_NOZORDER);

            SelectObject(hdc, old_font);
            ReleaseDC(button, hdc);
        }
    }

    return (NGHandle)button;
}

void ng_windows_button_invalidate(NGHandle button) {
    if (!button) return;
    InvalidateRect((HWND)button, NULL, FALSE);
}
