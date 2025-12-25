#include "utils.h"
#include "common/errors.h"

static WNDCLASSEXA g_wc = {0};
static const char* CLASS_NAME = "NativeGuiWindow";
static BOOL win_initialized = FALSE;

LRESULT CALLBACK WindowProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam);

int ng_windows_init(void) {
    if (!win_initialized) {
        g_wc.cbSize = sizeof(WNDCLASSEXA);
        g_wc.lpfnWndProc = WindowProc;
        g_wc.hInstance = GetModuleHandleA(NULL);
        g_wc.lpszClassName = CLASS_NAME;
        g_wc.hCursor = LoadCursor(NULL, IDC_ARROW);
        g_wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
        
        if (!RegisterClassExA(&g_wc)) {
            return NG_ERROR_PLATFORM_SPECIFIC;
        }
        win_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_windows_cleanup(void) {
    if (win_initialized) {
        UnregisterClassA(CLASS_NAME, g_wc.hInstance);
        win_initialized = FALSE;
    }
}

BOOL ng_windows_is_initialized(void) {
    return win_initialized;
}

const char* ng_windows_get_class_name(void) {
    return CLASS_NAME;
}

extern void ng_invoke_menu_callback(unsigned int id);
extern void ng_invoke_button_callback(unsigned int id);

LRESULT CALLBACK WindowProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam) {
    switch (uMsg) {
        case WM_COMMAND:
            if (HIWORD(wParam) == 0) {
                unsigned int command_id = LOWORD(wParam);
                if (command_id >= 1000) {
                    unsigned int id = command_id - 1000;
                    ng_invoke_button_callback(id);
                } else {
                    unsigned int id = command_id - 1;
                    ng_invoke_menu_callback(id);
                }
            }
            break;
        
        case WM_DESTROY:
            PostQuitMessage(0);
            return 0;
    }
    return DefWindowProcA(hwnd, uMsg, wParam, lParam);
}

