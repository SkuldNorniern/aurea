#include "utils.h"
#include "common/errors.h"
#include "common/input.h"
#include "common/rust_callbacks.h"
#include <windowsx.h>

#define AUREA_CURSOR_GRAB_PROP "AureaCursorGrabMode"

static WNDCLASSEXA g_wc = {0};
static const char* CLASS_NAME = "NativeGuiWindow";
static BOOL win_initialized = FALSE;

static ScaleFactorCallback g_window_scale_callbacks[256] = {0};
static HWND g_tracked_windows[256] = {0};
static int g_tracked_count = 0;
static BOOL g_lifecycle_callbacks[256] = {0};
static BOOL g_mouse_inside[256] = {0};

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

void ng_windows_register_lifecycle_callback(HWND hwnd) {
    for (int i = 0; i < g_tracked_count && i < 256; i++) {
        if (g_tracked_windows[i] == hwnd) {
            g_lifecycle_callbacks[i] = TRUE;
            return;
        }
    }
    // If window not tracked, add it
    if (g_tracked_count < 256) {
        g_tracked_windows[g_tracked_count] = hwnd;
        g_lifecycle_callbacks[g_tracked_count] = TRUE;
        g_tracked_count++;
    }
}

static unsigned int ng_windows_modifiers(void) {
    unsigned int mods = 0;
    if (GetKeyState(VK_SHIFT) & 0x8000) {
        mods |= NG_MOD_SHIFT;
    }
    if (GetKeyState(VK_CONTROL) & 0x8000) {
        mods |= NG_MOD_CTRL;
    }
    if (GetKeyState(VK_MENU) & 0x8000) {
        mods |= NG_MOD_ALT;
    }
    if (GetKeyState(VK_LWIN) & 0x8000 || GetKeyState(VK_RWIN) & 0x8000) {
        mods |= NG_MOD_META;
    }
    return mods;
}

static unsigned int ng_windows_keycode_from_vk(WPARAM vk) {
    if (vk >= 'A' && vk <= 'Z') {
        return NG_KEY_A + (unsigned int)(vk - 'A');
    }
    if (vk >= '0' && vk <= '9') {
        return NG_KEY_0 + (unsigned int)(vk - '0');
    }

    switch (vk) {
        case VK_SPACE:
            return NG_KEY_SPACE;
        case VK_RETURN:
            return NG_KEY_ENTER;
        case VK_ESCAPE:
            return NG_KEY_ESCAPE;
        case VK_TAB:
            return NG_KEY_TAB;
        case VK_BACK:
            return NG_KEY_BACKSPACE;
        case VK_DELETE:
            return NG_KEY_DELETE;
        case VK_INSERT:
            return NG_KEY_INSERT;
        case VK_HOME:
            return NG_KEY_HOME;
        case VK_END:
            return NG_KEY_END;
        case VK_PRIOR:
            return NG_KEY_PAGE_UP;
        case VK_NEXT:
            return NG_KEY_PAGE_DOWN;
        case VK_UP:
            return NG_KEY_UP;
        case VK_DOWN:
            return NG_KEY_DOWN;
        case VK_LEFT:
            return NG_KEY_LEFT;
        case VK_RIGHT:
            return NG_KEY_RIGHT;
        case VK_F1:
            return NG_KEY_F1;
        case VK_F2:
            return NG_KEY_F2;
        case VK_F3:
            return NG_KEY_F3;
        case VK_F4:
            return NG_KEY_F4;
        case VK_F5:
            return NG_KEY_F5;
        case VK_F6:
            return NG_KEY_F6;
        case VK_F7:
            return NG_KEY_F7;
        case VK_F8:
            return NG_KEY_F8;
        case VK_F9:
            return NG_KEY_F9;
        case VK_F10:
            return NG_KEY_F10;
        case VK_F11:
            return NG_KEY_F11;
        case VK_F12:
            return NG_KEY_F12;
        case VK_SHIFT:
        case VK_LSHIFT:
        case VK_RSHIFT:
            return NG_KEY_SHIFT;
        case VK_CONTROL:
        case VK_LCONTROL:
        case VK_RCONTROL:
            return NG_KEY_CONTROL;
        case VK_MENU:
        case VK_LMENU:
        case VK_RMENU:
            return NG_KEY_ALT;
        case VK_LWIN:
        case VK_RWIN:
            return NG_KEY_META;
        default:
            return NG_KEY_UNKNOWN;
    }
}

static void ng_windows_emit_text_input(HWND hwnd, wchar_t wc) {
    char buffer[8];
    int len = WideCharToMultiByte(CP_UTF8, 0, &wc, 1, buffer, (int)sizeof(buffer) - 1, NULL, NULL);
    if (len > 0) {
        buffer[len] = '\0';
        ng_invoke_text_input((void*)hwnd, buffer);
    }
}

LRESULT CALLBACK WindowProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam) {
    switch (uMsg) {
        case WM_SETFOCUS:
            ng_invoke_focus_changed((void*)hwnd, 1);
            break;
        case WM_KILLFOCUS:
            ng_invoke_focus_changed((void*)hwnd, 0);
            break;
        case WM_MOUSEMOVE: {
            int idx = -1;
            for (int i = 0; i < g_tracked_count; i++) {
                if (g_tracked_windows[i] == hwnd) {
                    idx = i;
                    break;
                }
            }
            if (idx >= 0 && !g_mouse_inside[idx]) {
                g_mouse_inside[idx] = TRUE;
                ng_invoke_cursor_entered((void*)hwnd, 1);
            }
            TRACKMOUSEEVENT tme = {0};
            tme.cbSize = sizeof(TRACKMOUSEEVENT);
            tme.dwFlags = TME_LEAVE;
            tme.hwndTrack = hwnd;
            TrackMouseEvent(&tme);

            double x = (double)GET_X_LPARAM(lParam);
            double y = (double)GET_Y_LPARAM(lParam);
            ng_invoke_mouse_move((void*)hwnd, x, y);
            break;
        }
        case WM_MOUSELEAVE: {
            int idx = -1;
            for (int i = 0; i < g_tracked_count; i++) {
                if (g_tracked_windows[i] == hwnd) {
                    idx = i;
                    break;
                }
            }
            if (idx >= 0) {
                g_mouse_inside[idx] = FALSE;
            }
            ng_invoke_cursor_entered((void*)hwnd, 0);
            break;
        }
        default:
            break;
    }

    switch (uMsg) {
        case WM_INPUT: {
            HANDLE prop = GetPropA(hwnd, AUREA_CURSOR_GRAB_PROP);
            if ((INT_PTR)prop == 2) {
                RAWINPUT raw;
                UINT size = sizeof(raw);
                if (GetRawInputData((HRAWINPUT)lParam, RID_INPUT, &raw, &size, sizeof(RAWINPUTHEADER)) == size) {
                    if (raw.header.dwType == RIM_TYPEMOUSE) {
                        ng_invoke_raw_mouse_motion(
                            (void*)hwnd,
                            (double)raw.data.mouse.lLastX,
                            (double)raw.data.mouse.lLastY);
                    }
                }
            }
            break;
        }
        case WM_KEYDOWN:
        case WM_SYSKEYDOWN: {
            unsigned int keycode = ng_windows_keycode_from_vk(wParam);
            ng_invoke_key_event((void*)hwnd, keycode, 1, ng_windows_modifiers());
            break;
        }
        case WM_KEYUP:
        case WM_SYSKEYUP: {
            unsigned int keycode = ng_windows_keycode_from_vk(wParam);
            ng_invoke_key_event((void*)hwnd, keycode, 0, ng_windows_modifiers());
            break;
        }
        case WM_CHAR:
        case WM_SYSCHAR: {
            wchar_t wc = (wchar_t)wParam;
            ng_windows_emit_text_input(hwnd, wc);
            break;
        }
        case WM_LBUTTONDOWN:
            ng_invoke_mouse_button((void*)hwnd, 0, 1, ng_windows_modifiers());
            break;
        case WM_LBUTTONUP:
            ng_invoke_mouse_button((void*)hwnd, 0, 0, ng_windows_modifiers());
            break;
        case WM_RBUTTONDOWN:
            ng_invoke_mouse_button((void*)hwnd, 1, 1, ng_windows_modifiers());
            break;
        case WM_RBUTTONUP:
            ng_invoke_mouse_button((void*)hwnd, 1, 0, ng_windows_modifiers());
            break;
        case WM_MBUTTONDOWN:
            ng_invoke_mouse_button((void*)hwnd, 2, 1, ng_windows_modifiers());
            break;
        case WM_MBUTTONUP:
            ng_invoke_mouse_button((void*)hwnd, 2, 0, ng_windows_modifiers());
            break;
        case WM_XBUTTONDOWN: {
            int button = (GET_XBUTTON_WPARAM(wParam) == XBUTTON1) ? 3 : 4;
            ng_invoke_mouse_button((void*)hwnd, button, 1, ng_windows_modifiers());
            break;
        }
        case WM_XBUTTONUP: {
            int button = (GET_XBUTTON_WPARAM(wParam) == XBUTTON1) ? 3 : 4;
            ng_invoke_mouse_button((void*)hwnd, button, 0, ng_windows_modifiers());
            break;
        }
        case WM_MOUSEWHEEL: {
            double delta = (double)GET_WHEEL_DELTA_WPARAM(wParam) / (double)WHEEL_DELTA;
            ng_invoke_mouse_wheel((void*)hwnd, 0.0, delta, ng_windows_modifiers());
            break;
        }
        case WM_MOUSEHWHEEL: {
            double delta = (double)GET_WHEEL_DELTA_WPARAM(wParam) / (double)WHEEL_DELTA;
            ng_invoke_mouse_wheel((void*)hwnd, delta, 0.0, ng_windows_modifiers());
            break;
        }
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
        
        case WM_CLOSE: {
            // Invoke lifecycle callback before closing
            for (int i = 0; i < g_tracked_count; i++) {
                if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                    ng_invoke_lifecycle_callback((void*)hwnd, 5); // WindowWillClose = 5
                    break;
                }
            }
            // Continue with default close behavior
            break;
        }
        
        case WM_SIZE: {
            // Check for minimize/restore
            if (wParam == SIZE_MINIMIZED) {
                for (int i = 0; i < g_tracked_count; i++) {
                    if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                        ng_invoke_lifecycle_callback((void*)hwnd, 6); // WindowMinimized = 6
                        ng_invoke_lifecycle_callback((void*)hwnd, 9); // SurfaceLost = 9
                        break;
                    }
                }
            } else if (wParam == SIZE_RESTORED || wParam == SIZE_MAXIMIZED) {
                for (int i = 0; i < g_tracked_count; i++) {
                    if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                        ng_invoke_lifecycle_callback((void*)hwnd, 7); // WindowRestored = 7
                        ng_invoke_lifecycle_callback((void*)hwnd, 10); // SurfaceRecreated = 10
                        break;
                    }
                }
            }
            if (wParam != SIZE_MINIMIZED) {
                for (int i = 0; i < g_tracked_count; i++) {
                    if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                        ng_invoke_lifecycle_callback((void*)hwnd, 12); // WindowResized = 12
                        break;
                    }
                }
            }
            break;
        }
        case WM_MOVE: {
            for (int i = 0; i < g_tracked_count; i++) {
                if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                    ng_invoke_lifecycle_callback((void*)hwnd, 11); // WindowMoved = 11
                    break;
                }
            }
            break;
        }
        
        case WM_DESTROY:
            PostQuitMessage(0);
            return 0;
    }
    return DefWindowProcA(hwnd, uMsg, wParam, lParam);
}
