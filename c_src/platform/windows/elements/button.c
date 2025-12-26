#include "common.h"
#include "../elements.h"
#include "../../common/errors.h"
#include <windows.h>

extern void ng_invoke_button_callback(unsigned int id);

NGHandle ng_windows_create_button(const char* title, unsigned int id) {
    if (!title) return NULL;

    HWND temp_parent = GetDesktopWindow();
    UINT command_id = id + 1000;

    HWND button = CreateWindowExA(
        0,
        "BUTTON",
        title,
        WS_CHILD | BS_PUSHBUTTON | WS_VISIBLE,
        0, 0, BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT,
        temp_parent,
        (HMENU)(UINT_PTR)command_id,
        GetModuleHandleA(NULL),
        NULL
    );

    if (button) {
        HDC hdc = GetDC(button);
        if (hdc) {
            HFONT hfont = (HFONT)SendMessageA(button, WM_GETFONT, 0, 0);
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

