#include "utils.h"
#include "common/errors.h"

static WNDCLASSEXA g_wc = {0};
static const char* CLASS_NAME = "NativeGuiWindow";
static BOOL win_initialized = FALSE;

static ScaleFactorCallback g_window_scale_callbacks[256] = {0};
static HWND g_tracked_windows[256] = {0};
static int g_tracked_count = 0;

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

void ng_windows_register_scale_callback(HWND hwnd, ScaleFactorCallback callback) {
    for (int i = 0; i < g_tracked_count && i < 256; i++) {
        if (g_tracked_windows[i] == hwnd) {
            g_window_scale_callbacks[i] = callback;
            return;
        }
    }
    if (g_tracked_count < 256) {
        g_tracked_windows[g_tracked_count] = hwnd;
        g_window_scale_callbacks[g_tracked_count] = callback;
        g_tracked_count++;
    }
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
        
        case WM_DPICHANGED: {
            // Windows 10+ DPI change notification
            UINT newDpiX = LOWORD(wParam);
            UINT newDpiY = HIWORD(wParam);
            float scale = (float)newDpiX / 96.0f;
            
            for (int i = 0; i < g_tracked_count; i++) {
                if (g_tracked_windows[i] == hwnd && g_window_scale_callbacks[i]) {
                    g_window_scale_callbacks[i]((void*)hwnd, scale);
                    break;
                }
            }
            
            // Apply the suggested window rect from lParam
            RECT* suggestedRect = (RECT*)lParam;
            SetWindowPos(hwnd, NULL,
                suggestedRect->left, suggestedRect->top,
                suggestedRect->right - suggestedRect->left,
                suggestedRect->bottom - suggestedRect->top,
                SWP_NOZORDER | SWP_NOACTIVATE);
            return 0;
        }
        
        case WM_DESTROY:
            PostQuitMessage(0);
            return 0;
    }
    return DefWindowProcA(hwnd, uMsg, wParam, lParam);
}

