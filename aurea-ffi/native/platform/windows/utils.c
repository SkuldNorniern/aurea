#include <stdlib.h>
#include "drop.h"
#include "utils.h"
#include "window.h"
#include "menu.h"
#include "ime.h"
#include "pen.h"
#include "common/shortcuts.h"
#include "elements/common.h"
#include "common/errors.h"
#include "common/input.h"
#include "common/rust_callbacks.h"
#include <windowsx.h>
#include <string.h>

#define AUREA_CURSOR_GRAB_PROP "AureaCursorGrabMode"

static WNDCLASSEXW g_wc = {0};
static const wchar_t* CLASS_NAME = L"NativeGuiWindow";
static BOOL win_initialized = FALSE;

static ScaleFactorCallback g_window_scale_callbacks[256] = {0};
static HWND g_tracked_windows[256] = {0};
static int g_tracked_count = 0;
static BOOL g_lifecycle_callbacks[256] = {0};
static BOOL g_mouse_inside[256] = {0};

LRESULT CALLBACK WindowProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam);

static void ng_windows_declare_dpi_awareness(void) {
    typedef BOOL (WINAPI *SetContextFunc)(HANDLE);
    typedef HRESULT (WINAPI *SetAwarenessFunc)(int);
    HMODULE user32 = GetModuleHandleA("user32.dll");
    SetContextFunc set_context = user32
        ? (SetContextFunc)GetProcAddress(user32, "SetProcessDpiAwarenessContext")
        : NULL;
    if (set_context) {
        if (set_context((HANDLE)(INT_PTR)-4)) return;
        if (GetLastError() == ERROR_ACCESS_DENIED) return;
    }
    HMODULE shcore = LoadLibraryA("shcore.dll");
    SetAwarenessFunc set_awareness = shcore
        ? (SetAwarenessFunc)GetProcAddress(shcore, "SetProcessDpiAwareness")
        : NULL;
    if (set_awareness) {
        HRESULT hr = set_awareness(2);
        if (SUCCEEDED(hr) || hr == E_ACCESSDENIED) return;
    }
    SetProcessDPIAware();
}

int ng_windows_init(void) {
    if (!win_initialized) {
        ng_windows_declare_dpi_awareness();
        g_wc.cbSize = sizeof(WNDCLASSEXW);
        g_wc.style = CS_DBLCLKS;
        g_wc.lpfnWndProc = WindowProc;
        g_wc.hInstance = GetModuleHandleA(NULL);
        g_wc.lpszClassName = CLASS_NAME;
        g_wc.hCursor = LoadCursor(NULL, IDC_ARROW);
        g_wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
        
        if (!RegisterClassExW(&g_wc)) {
            return NG_ERROR_PLATFORM_SPECIFIC;
        }
        win_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_windows_cleanup(void) {
    if (win_initialized) {
        UnregisterClassW(CLASS_NAME, g_wc.hInstance);
        win_initialized = FALSE;
    }
}

BOOL ng_windows_is_initialized(void) {
    return win_initialized;
}

wchar_t* ng_windows_utf8_to_wide(const char* utf8) {
    if (!utf8) return NULL;
    int count = MultiByteToWideChar(CP_UTF8, 0, utf8, -1, NULL, 0);
    if (count <= 0) return NULL;
    wchar_t* wide = (wchar_t*)malloc((size_t)count * sizeof(wchar_t));
    if (!wide) return NULL;
    if (MultiByteToWideChar(CP_UTF8, 0, utf8, -1, wide, count) <= 0) {
        free(wide);
        return NULL;
    }
    return wide;
}

const wchar_t* ng_windows_get_class_name(void) {
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
    if (g_tracked_count < 256) {
        g_tracked_windows[g_tracked_count] = hwnd;
        g_lifecycle_callbacks[g_tracked_count] = TRUE;
        g_tracked_count++;
    }
}

unsigned int ng_windows_window_refresh_hz(void) {
    DWORD best = 0;
    for (int i = 0; i < g_tracked_count; i++) {
        HMONITOR monitor = MonitorFromWindow(g_tracked_windows[i], MONITOR_DEFAULTTONEAREST);
        MONITORINFOEXW info;
        ZeroMemory(&info, sizeof(info));
        info.cbSize = sizeof(info);
        if (!monitor || !GetMonitorInfoW(monitor, (MONITORINFO*)&info)) continue;
        DEVMODEW mode;
        ZeroMemory(&mode, sizeof(mode));
        mode.dmSize = sizeof(mode);
        if (EnumDisplaySettingsW(info.szDevice, ENUM_CURRENT_SETTINGS, &mode)
            && mode.dmDisplayFrequency > 1 && mode.dmDisplayFrequency > best) {
            best = mode.dmDisplayFrequency;
        }
    }
    return (unsigned int)best;
}

void ng_windows_present(void) {
    for (int i = 0; i < g_tracked_count; i++) {
        RedrawWindow(g_tracked_windows[i], NULL, NULL, RDW_UPDATENOW | RDW_ALLCHILDREN);
    }
}

unsigned int ng_windows_modifiers(void) {
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
        case VK_OEM_MINUS:
            return NG_KEY_MINUS;
        case VK_OEM_PLUS:
            return NG_KEY_EQUALS;
        case VK_OEM_4:
            return NG_KEY_LEFT_BRACKET;
        case VK_OEM_6:
            return NG_KEY_RIGHT_BRACKET;
        case VK_OEM_5:
            return NG_KEY_BACKSLASH;
        case VK_OEM_1:
            return NG_KEY_SEMICOLON;
        case VK_OEM_7:
            return NG_KEY_APOSTROPHE;
        case VK_OEM_3:
            return NG_KEY_GRAVE;
        case VK_OEM_COMMA:
            return NG_KEY_COMMA;
        case VK_OEM_PERIOD:
            return NG_KEY_PERIOD;
        case VK_OEM_2:
            return NG_KEY_SLASH;
        default:
            return NG_KEY_UNKNOWN;
    }
}

int ng_windows_window_set_shortcuts(NGHandle window, const unsigned int* keycodes, const unsigned int* modifiers, int count) {
    return ng_shortcuts_set(window, keycodes, modifiers, count);
}

int ng_windows_take_shortcut(HWND root, WPARAM vk) {
    unsigned int keycode = ng_windows_keycode_from_vk(vk);
    unsigned int mods = ng_windows_modifiers();
    if (!ng_shortcuts_match((void*)root, keycode, mods)) return 0;
    ng_invoke_key_event((void*)root, keycode, 1, mods);
    return 1;
}

#define AUREA_HIGH_SURROGATE_PROP "AureaHighSurrogate"

static void ng_windows_emit_text_input(HWND hwnd, wchar_t wc) {
    if (wc >= 0xD800 && wc <= 0xDBFF) {
        SetPropA(hwnd, AUREA_HIGH_SURROGATE_PROP, (HANDLE)(UINT_PTR)wc);
        return;
    }

    wchar_t high = (wchar_t)(UINT_PTR)RemovePropA(hwnd, AUREA_HIGH_SURROGATE_PROP);
    wchar_t units[2];
    int count;
    if (high != 0 && wc >= 0xDC00 && wc <= 0xDFFF) {
        units[0] = high;
        units[1] = wc;
        count = 2;
    } else {
        units[0] = wc;
        count = 1;
    }

    char buffer[8];
    int len = WideCharToMultiByte(CP_UTF8, 0, units, count, buffer, (int)sizeof(buffer) - 1, NULL, NULL);
    if (len > 0) {
        buffer[len] = '\0';
        ng_invoke_text_input((void*)hwnd, buffer);
    }
}

double ng_windows_logical(HWND hwnd, int physical) {
    float scale = ng_windows_get_scale_factor((NGHandle)hwnd);
    return (double)physical / (scale > 1.0f ? (double)scale : 1.0);
}

static void ng_windows_emit_mouse_button(
    HWND hwnd,
    LPARAM lParam,
    int button,
    int pressed,
    int click_count
) {
    ng_invoke_mouse_button(
        (void*)hwnd,
        button,
        pressed,
        ng_windows_modifiers(),
        ng_windows_logical(hwnd, GET_X_LPARAM(lParam)),
        ng_windows_logical(hwnd, GET_Y_LPARAM(lParam)),
        click_count);
}

static unsigned int ng_windows_buttons(WPARAM held) {
    unsigned int buttons = 0;
    if (held & MK_LBUTTON) buttons |= 1u << 0;
    if (held & MK_RBUTTON) buttons |= 1u << 1;
    if (held & MK_MBUTTON) buttons |= 1u << 2;
    if (held & MK_XBUTTON1) buttons |= 1u << 3;
    if (held & MK_XBUTTON2) buttons |= 1u << 4;
    return buttons;
}

static int ng_windows_tracked_index(HWND hwnd) {
    for (int i = 0; i < g_tracked_count; i++) {
        if (g_tracked_windows[i] == hwnd) return i;
    }
    return -1;
}

#define AUREA_HOVER_TIMER_ID 0xA0E1
#define AUREA_ANY_BUTTON (MK_LBUTTON | MK_RBUTTON | MK_MBUTTON | MK_XBUTTON1 | MK_XBUTTON2)

static void ng_windows_track_leave(HWND target) {
    TRACKMOUSEEVENT tme = {0};
    tme.cbSize = sizeof(TRACKMOUSEEVENT);
    tme.dwFlags = TME_LEAVE;
    tme.hwndTrack = target;
    TrackMouseEvent(&tme);
}

#define AUREA_TRACKING_PROP "AureaTracking"

static BOOL ng_windows_pointer_in_content(HWND root, HWND* under) {
    POINT pt;
    GetCursorPos(&pt);
    *under = WindowFromPoint(pt);
    POINT client = pt;
    ScreenToClient(root, &client);
    RECT content;
    GetClientRect(root, &content);
    return PtInRect(&content, client) && *under && (*under == root || IsChild(root, *under));
}

static void ng_windows_arm_tracking(HWND root) {
    if (GetPropA(root, AUREA_TRACKING_PROP)) return;
    HWND under;
    if (!ng_windows_pointer_in_content(root, &under)) {
        SetTimer(root, AUREA_HOVER_TIMER_ID, 50, NULL);
        return;
    }
    HWND target = root;
    if (under != root) {
        char cls[32];
        GetClassNameA(under, cls, sizeof(cls));
        if (strcmp(cls, "AureaCanvas") != 0) {
            SetTimer(root, AUREA_HOVER_TIMER_ID, 50, NULL);
            return;
        }
        target = under;
    }
    KillTimer(root, AUREA_HOVER_TIMER_ID);
    SetPropA(root, AUREA_TRACKING_PROP, (HANDLE)1);
    ng_windows_track_leave(target);
}

static void ng_windows_recheck_pointer(HWND root) {
    int idx = ng_windows_tracked_index(root);
    if (idx < 0 || !g_mouse_inside[idx] || GetCapture() == root) return;

    HWND under;
    if (ng_windows_pointer_in_content(root, &under)) {
        if (!GetPropA(root, AUREA_TRACKING_PROP)) {
            SetTimer(root, AUREA_HOVER_TIMER_ID, 50, NULL);
        }
        return;
    }

    KillTimer(root, AUREA_HOVER_TIMER_ID);
    g_mouse_inside[idx] = FALSE;
    ng_invoke_cursor_entered((void*)root, 0);
}

static void ng_windows_release_capture(HWND hwnd, WPARAM held) {
    if (!(held & AUREA_ANY_BUTTON) && GetCapture() == hwnd) {
        ReleaseCapture();
    }
}

LRESULT CALLBACK WindowProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam) {
    LRESULT ime_result;
    if (ng_windows_ime_message(hwnd, uMsg, wParam, lParam, &ime_result)) {
        return ime_result;
    }
    switch (uMsg) {
        case WM_SETFOCUS:
            ng_invoke_focus_changed((void*)hwnd, 1);
            break;
        case WM_ACTIVATE:
            ng_windows_cursor_grab_activate((void*)hwnd, LOWORD(wParam) != WA_INACTIVE);
            break;
        case WM_KILLFOCUS:
            RemovePropA(hwnd, AUREA_HIGH_SURROGATE_PROP);
            ng_invoke_focus_changed((void*)hwnd, 0);
            break;
        case WM_MOUSEMOVE: {
            int idx = ng_windows_tracked_index(hwnd);
            if (idx >= 0 && !g_mouse_inside[idx]) {
                g_mouse_inside[idx] = TRUE;
                ng_invoke_cursor_entered((void*)hwnd, 1);
            }
            if (idx >= 0) ng_windows_arm_tracking(hwnd);

            ng_invoke_mouse_move(
                (void*)hwnd,
                ng_windows_logical(hwnd, GET_X_LPARAM(lParam)),
                ng_windows_logical(hwnd, GET_Y_LPARAM(lParam)),
                ng_windows_buttons(wParam),
                ng_windows_modifiers());
            break;
        }
        case WM_POINTERDOWN:
        case WM_POINTERUPDATE:
        case WM_POINTERUP:
            ng_windows_pen_message(hwnd, uMsg, wParam);
            break;
        case WM_MOUSELEAVE:
            RemovePropA(hwnd, AUREA_TRACKING_PROP);
            ng_windows_recheck_pointer(hwnd);
            break;
        case WM_SETCURSOR:
            if (LOWORD(lParam) == HTCLIENT && (HWND)wParam == hwnd) {
                SetCursor((HCURSOR)ng_windows_window_cursor((NGHandle)hwnd));
                return TRUE;
            }
            break;
        case WM_TIMER:
            if (wParam == AUREA_HOVER_TIMER_ID) {
                ng_windows_recheck_pointer(hwnd);
                int idx = ng_windows_tracked_index(hwnd);
                if (idx >= 0 && g_mouse_inside[idx]) ng_windows_arm_tracking(hwnd);
                return 0;
            }
            if (wParam == AUREA_FRAME_TIMER_ID) {
                ng_windows_modal_tick();
                return 0;
            }
            break;
        case WM_ENTERSIZEMOVE:
        case WM_ENTERMENULOOP:
            ng_windows_modal_begin(hwnd);
            break;
        case WM_SIZING:
        case WM_MOVING:
        case WM_MENUSELECT:
            ng_windows_modal_tick();
            break;
        case WM_EXITSIZEMOVE:
        case WM_EXITMENULOOP:
            ng_windows_modal_end(hwnd);
            break;
        case WM_DISPLAYCHANGE:
        case WM_SHOWWINDOW:
            ng_windows_display_changed();
            break;
        case WM_LBUTTONDOWN:
        case WM_LBUTTONDBLCLK:
        case WM_RBUTTONDOWN:
        case WM_RBUTTONDBLCLK:
        case WM_MBUTTONDOWN:
        case WM_MBUTTONDBLCLK:
        case WM_XBUTTONDOWN:
        case WM_XBUTTONDBLCLK:
            SetCapture(hwnd);
            break;
        case WM_CAPTURECHANGED:
            if ((HWND)lParam != hwnd) {
                ng_windows_recheck_pointer(hwnd);
            }
            break;
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
            unsigned int modifiers = ng_windows_modifiers();
            ng_invoke_key_event((void*)hwnd, keycode, 1, modifiers);
            if (ng_windows_handle_menu_shortcut((void*)hwnd, keycode, modifiers)) {
                return 0;
            }
            if (wParam == VK_MENU || wParam == VK_F10) {
                return 0;
            }
            break;
        }
        case WM_KEYUP:
        case WM_SYSKEYUP: {
            unsigned int keycode = ng_windows_keycode_from_vk(wParam);
            ng_invoke_key_event((void*)hwnd, keycode, 0, ng_windows_modifiers());
            if (wParam == VK_MENU || wParam == VK_F10) {
                return 0;
            }
            break;
        }
        case WM_CHAR: {
            wchar_t wc = (wchar_t)wParam;
            ng_windows_emit_text_input(hwnd, wc);
            break;
        }
        case WM_LBUTTONDOWN:
            ng_windows_emit_mouse_button(hwnd, lParam, 0, 1, 1);
            break;
        case WM_LBUTTONDBLCLK:
            ng_windows_emit_mouse_button(hwnd, lParam, 0, 1, 2);
            break;
        case WM_LBUTTONUP:
            ng_windows_emit_mouse_button(hwnd, lParam, 0, 0, 1);
            ng_windows_release_capture(hwnd, wParam);
            break;
        case WM_RBUTTONDOWN:
            ng_windows_emit_mouse_button(hwnd, lParam, 1, 1, 1);
            break;
        case WM_RBUTTONDBLCLK:
            ng_windows_emit_mouse_button(hwnd, lParam, 1, 1, 2);
            break;
        case WM_RBUTTONUP:
            ng_windows_emit_mouse_button(hwnd, lParam, 1, 0, 1);
            ng_windows_release_capture(hwnd, wParam);
            break;
        case WM_MBUTTONDOWN:
            ng_windows_emit_mouse_button(hwnd, lParam, 2, 1, 1);
            break;
        case WM_MBUTTONDBLCLK:
            ng_windows_emit_mouse_button(hwnd, lParam, 2, 1, 2);
            break;
        case WM_MBUTTONUP:
            ng_windows_emit_mouse_button(hwnd, lParam, 2, 0, 1);
            ng_windows_release_capture(hwnd, wParam);
            break;
        case WM_XBUTTONDOWN: {
            int button = (GET_XBUTTON_WPARAM(wParam) == XBUTTON1) ? 3 : 4;
            ng_windows_emit_mouse_button(hwnd, lParam, button, 1, 1);
            break;
        }
        case WM_XBUTTONDBLCLK: {
            int button = (GET_XBUTTON_WPARAM(wParam) == XBUTTON1) ? 3 : 4;
            ng_windows_emit_mouse_button(hwnd, lParam, button, 1, 2);
            break;
        }
        case WM_XBUTTONUP: {
            int button = (GET_XBUTTON_WPARAM(wParam) == XBUTTON1) ? 3 : 4;
            ng_windows_emit_mouse_button(hwnd, lParam, button, 0, 1);
            ng_windows_release_capture(hwnd, wParam);
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
                if (command_id >= BUTTON_COMMAND_BASE) {
                    unsigned int id = command_id - BUTTON_COMMAND_BASE;
                    ng_invoke_button_callback(id);
                } else {
                    unsigned int id = command_id - 1;
                    ng_invoke_menu_callback(id);
                }
            }
            break;
        
        case WM_DPICHANGED: {
            UINT newDpiX = LOWORD(wParam);
            UINT newDpiY = HIWORD(wParam);
            float scale = (float)newDpiX / 96.0f;
            
            for (int i = 0; i < g_tracked_count; i++) {
                if (g_tracked_windows[i] == hwnd && g_window_scale_callbacks[i]) {
                    g_window_scale_callbacks[i]((void*)hwnd, scale);
                    break;
                }
            }
            
            RECT* suggestedRect = (RECT*)lParam;
            SetWindowPos(hwnd, NULL,
                suggestedRect->left, suggestedRect->top,
                suggestedRect->right - suggestedRect->left,
                suggestedRect->bottom - suggestedRect->top,
                SWP_NOZORDER | SWP_NOACTIVATE);
            ng_windows_refresh_dpi(hwnd);
            return 0;
        }
        
        case WM_CLOSE: {
            for (int i = 0; i < g_tracked_count; i++) {
                if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                    ng_invoke_lifecycle_callback((void*)hwnd, 5);
                    break;
                }
            }
            if (GetPropA(hwnd, AUREA_CLOSE_GUARD_PROPERTY)) {
                return 0;
            }
            if (GetPropA(hwnd, AUREA_HIDE_ON_CLOSE_PROPERTY)) {
                ShowWindow(hwnd, SW_HIDE);
                return 0;
            }
            break;
        }
        
        case WM_SIZE: {
            ng_windows_cursor_grab_refresh((void*)hwnd);
            BOOL was_minimized = GetPropA(hwnd, "AureaMinimized") != NULL;
            if (wParam == SIZE_MINIMIZED) {
                if (was_minimized) break;
                SetPropA(hwnd, "AureaMinimized", (HANDLE)1);
                for (int i = 0; i < g_tracked_count; i++) {
                    if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                        ng_invoke_lifecycle_callback((void*)hwnd, 6);
                        ng_invoke_lifecycle_callback((void*)hwnd, 9);
                        break;
                    }
                }
            } else {
                ng_windows_fit_content(hwnd);

                if (was_minimized) {
                    RemovePropA(hwnd, "AureaMinimized");
                    for (int i = 0; i < g_tracked_count; i++) {
                        if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                            ng_invoke_lifecycle_callback((void*)hwnd, 7);
                            ng_invoke_lifecycle_callback((void*)hwnd, 10);
                            break;
                        }
                    }
                }
                for (int i = 0; i < g_tracked_count; i++) {
                    if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                        ng_invoke_lifecycle_callback((void*)hwnd, 12);
                        break;
                    }
                }
            }
            break;
        }
        case WM_MOVE: {
            ng_windows_cursor_grab_refresh((void*)hwnd);
            HMONITOR monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if ((HMONITOR)GetPropA(hwnd, "AureaMonitor") != monitor) {
                SetPropA(hwnd, "AureaMonitor", (HANDLE)monitor);
                ng_windows_display_changed();
            }
            for (int i = 0; i < g_tracked_count; i++) {
                if (g_tracked_windows[i] == hwnd && g_lifecycle_callbacks[i]) {
                    ng_invoke_lifecycle_callback((void*)hwnd, 11);
                    break;
                }
            }
            break;
        }
        
        case WM_DESTROY: {
            ng_windows_release_window_input((void*)hwnd);
            ng_windows_refuse_drops(hwnd);
            RemovePropA(hwnd, "AureaMonitor");
            RemovePropA(hwnd, AUREA_TRACKING_PROP);
            RemovePropA(hwnd, AUREA_HIGH_SURROGATE_PROP);
            RemovePropA(hwnd, "AureaMinimized");
            ng_windows_ime_release(hwnd);
            ng_shortcuts_clear((void*)hwnd);
            for (int i = 0; i < g_tracked_count; i++) {
                if (g_tracked_windows[i] == hwnd) {
                    int last = g_tracked_count - 1;
                    g_tracked_windows[i] = g_tracked_windows[last];
                    g_window_scale_callbacks[i] = g_window_scale_callbacks[last];
                    g_lifecycle_callbacks[i] = g_lifecycle_callbacks[last];
                    g_mouse_inside[i] = g_mouse_inside[last];
                    g_tracked_windows[last] = NULL;
                    g_window_scale_callbacks[last] = NULL;
                    g_lifecycle_callbacks[last] = FALSE;
                    g_mouse_inside[last] = FALSE;
                    g_tracked_count--;
                    break;
                }
            }
            if (g_tracked_count == 0) {
                PostQuitMessage(0);
            }
            return 0;
        }
    }
    return DefWindowProcW(hwnd, uMsg, wParam, lParam);
}

int ng_windows_reduced_motion(void) {
    BOOL animate = TRUE;
    if (!SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &animate, 0)) return 0;
    return animate ? 0 : 1;
}

int ng_windows_locales(char* buf, int cap) {
    ULONG count = 0;
    ULONG len = 0;
    if (!GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &count, NULL, &len) || len < 2) return 0;
    WCHAR* wide = (WCHAR*)malloc(len * sizeof(WCHAR));
    if (!wide) return 0;
    int need = 0;
    if (GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &count, wide, &len) && len >= 2) {
        int chars = (int)len - 1;
        need = WideCharToMultiByte(CP_UTF8, 0, wide, chars, NULL, 0, NULL, NULL);
        if (buf && need > 0 && cap >= need) {
            WideCharToMultiByte(CP_UTF8, 0, wide, chars, buf, cap, NULL, NULL);
        }
    }
    free(wide);
    return need;
}

void ng_windows_fit_content(HWND window) {
    HWND content = (HWND)GetPropA(window, "AureaContentHWND");
    if (!content || !IsWindow(content)) return;
    RECT client;
    GetClientRect(window, &client);
    int w = client.right - client.left;
    int h = client.bottom - client.top;
    if (w <= 0 || h <= 0) return;
    SetWindowPos(content, NULL, 0, 0, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    extern void layout_box_children(HWND box);
    if (GetPropA(content, "AureaBoxOrientation")) {
        layout_box_children(content);
    }
}
