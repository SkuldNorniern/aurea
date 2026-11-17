#include <stdlib.h>
#include "drop.h"
#include "utils.h"
#include "occlusion.h"

#define AUREA_OCCLUSION_TIMER_ID 0xA0E3
#include "window.h"
#include "window_state.h"
#include "menu.h"
#include "ime.h"
#include "pen.h"
#include "gesture.h"
#include "text.h"
#include "raw.h"
#include "touch.h"
#include "common/shortcuts.h"
#include "elements/common.h"
#include "common/errors.h"
#include "common/input.h"
#include <commctrl.h>
#include "common/rust_callbacks.h"
#include <windowsx.h>
#include <string.h>

static WNDCLASSEXW g_wc = {0};
static const wchar_t* CLASS_NAME = L"NativeGuiWindow";

/* Every top-level window hears a theme broadcast; the app is told once. */
static int g_theme_dark = -1;
static void ng_windows_theme_check(void);
static BOOL win_initialized = FALSE;

LRESULT CALLBACK WindowProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam);

/* Tells Rust about `hwnd`'s lifecycle `code`, if it listens for them. */
static void ng_windows_lifecycle(HWND hwnd, int code) {
    NGWindow* w = ng_windows_window_state(hwnd);
    if (w && w->lifecycle) ng_invoke_lifecycle_callback((void*)hwnd, code);
}

/* Lifecycle codes 13 to 15: visible, partly covered, covered. Sent when the level changes. */
static void ng_windows_check_occlusion(HWND hwnd) {
    NGWindow* w = ng_windows_window_state(hwnd);
    if (!w) return;
    int now = ng_windows_occlusion_of(hwnd);
    int was = w->occlusion;
    if (now == was) return;
    w->occlusion = now;
    /* a window that starts out covered says so too */
    if (was >= 0 || now != AUREA_OCCLUSION_VISIBLE) ng_windows_lifecycle(hwnd, 13 + now);
}

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
    typedef BOOL (WINAPI *SetAwareFunc)(void);
    SetAwareFunc set_aware = (SetAwareFunc)ng_windows_api("user32.dll", "SetProcessDPIAware");
    if (set_aware) set_aware();
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
        ng_windows_activate_controls_v6();
        ng_windows_init_common_controls(ICC_STANDARD_CLASSES | ICC_WIN95_CLASSES | ICC_DATE_CLASSES |
                                        ICC_USEREX_CLASSES | ICC_COOL_CLASSES | ICC_INTERNET_CLASSES |
                                        ICC_PAGESCROLLER_CLASS | ICC_NATIVEFNTCTL_CLASS | ICC_LINK_CLASS);
        g_theme_dark = ng_windows_prefers_dark();
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

static FARPROC ng_windows_comctl(const char* name, WORD ordinal) {
    static HMODULE comctl = NULL;
    static int loaded = 0;
    if (!loaded) {
        loaded = 1;
        HANDLE v6 = ng_windows_common_controls_v6();
        ULONG_PTR cookie = 0;
        BOOL active = v6 != INVALID_HANDLE_VALUE && ActivateActCtx(v6, &cookie);
        comctl = LoadLibraryW(L"comctl32.dll");
        if (active) DeactivateActCtx(0, cookie);
    }
    if (!comctl) return NULL;
    FARPROC proc = GetProcAddress(comctl, name);
    return proc || !ordinal ? proc : GetProcAddress(comctl, MAKEINTRESOURCEA(ordinal));
}

BOOL WINAPI ng_windows_set_subclass(HWND hwnd, SUBCLASSPROC proc, UINT_PTR id, DWORD_PTR data) {
    typedef BOOL (WINAPI *Fn)(HWND, SUBCLASSPROC, UINT_PTR, DWORD_PTR);
    static Fn fn = NULL;
    if (!fn) fn = (Fn)ng_windows_comctl("SetWindowSubclass", 410);
    return fn ? fn(hwnd, proc, id, data) : FALSE;
}

BOOL WINAPI ng_windows_get_subclass(HWND hwnd, SUBCLASSPROC proc, UINT_PTR id, DWORD_PTR* data) {
    typedef BOOL (WINAPI *Fn)(HWND, SUBCLASSPROC, UINT_PTR, DWORD_PTR*);
    static Fn fn = NULL;
    if (!fn) fn = (Fn)ng_windows_comctl("GetWindowSubclass", 411);
    return fn ? fn(hwnd, proc, id, data) : FALSE;
}

BOOL WINAPI ng_windows_remove_subclass(HWND hwnd, SUBCLASSPROC proc, UINT_PTR id) {
    typedef BOOL (WINAPI *Fn)(HWND, SUBCLASSPROC, UINT_PTR);
    static Fn fn = NULL;
    if (!fn) fn = (Fn)ng_windows_comctl("RemoveWindowSubclass", 412);
    return fn ? fn(hwnd, proc, id) : FALSE;
}

LRESULT WINAPI ng_windows_def_subclass(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    typedef LRESULT (WINAPI *Fn)(HWND, UINT, WPARAM, LPARAM);
    static Fn fn = NULL;
    if (!fn) fn = (Fn)ng_windows_comctl("DefSubclassProc", 413);
    return fn ? fn(hwnd, msg, wParam, lParam) : DefWindowProcW(hwnd, msg, wParam, lParam);
}

/* The common controls v6 activation context, for the dialogs and the themed controls. */
HANDLE ng_windows_common_controls_v6(void) {
    static HANDLE context = NULL;
    if (context) return context;
    wchar_t dir[MAX_PATH];
    wchar_t shell32[MAX_PATH];
    UINT len = GetSystemDirectoryW(dir, MAX_PATH);
    if (len == 0 || len >= MAX_PATH - 12) {
        context = INVALID_HANDLE_VALUE;
        return context;
    }
    wcscpy_s(shell32, MAX_PATH, dir);
    wcscat_s(shell32, MAX_PATH, L"\\shell32.dll");
    ACTCTXW act;
    memset(&act, 0, sizeof act);
    act.cbSize = sizeof act;
    act.dwFlags = ACTCTX_FLAG_RESOURCE_NAME_VALID | ACTCTX_FLAG_ASSEMBLY_DIRECTORY_VALID;
    act.lpSource = shell32;
    act.lpAssemblyDirectory = dir;
    act.lpResourceName = MAKEINTRESOURCEW(124);
    context = CreateActCtxW(&act);
    return context;
}

/* Themed controls without an app manifest; it stays active for the thread. */
void ng_windows_activate_controls_v6(void) {
    static __declspec(thread) int active = 0;
    if (active) return;
    active = 1;
    HANDLE v6 = ng_windows_common_controls_v6();
    ULONG_PTR cookie = 0;
    if (v6 != INVALID_HANDLE_VALUE) ActivateActCtx(v6, &cookie);
}

BOOL ng_windows_init_common_controls(DWORD classes) {
    typedef BOOL (WINAPI *Fn)(const INITCOMMONCONTROLSEX*);
    static Fn fn = NULL;
    if (!fn) fn = (Fn)ng_windows_comctl("InitCommonControlsEx", 0);
    INITCOMMONCONTROLSEX init = {sizeof(init), classes};
    return fn ? fn(&init) : FALSE;
}

FARPROC ng_windows_api(const char* module, const char* name) {
    HMODULE dll = GetModuleHandleA(module);
    if (!dll) dll = LoadLibraryA(module);
    return dll ? GetProcAddress(dll, name) : NULL;
}

char* ng_windows_wide_to_utf8(const wchar_t* wide, int len) {
    if (!wide) return NULL;
    int bytes = len > 0 ? WideCharToMultiByte(CP_UTF8, 0, wide, len, NULL, 0, NULL, NULL) : 0;
    char* utf8 = (char*)malloc((size_t)bytes + 1);
    if (!utf8) return NULL;
    if (bytes > 0) WideCharToMultiByte(CP_UTF8, 0, wide, len, utf8, bytes, NULL, NULL);
    utf8[bytes] = '\0';
    return utf8;
}

char* ng_windows_window_text(HWND hwnd) {
    int len = GetWindowTextLengthW(hwnd);
    wchar_t* wide = (wchar_t*)malloc(((size_t)len + 1) * sizeof(wchar_t));
    if (!wide) return NULL;
    len = len > 0 ? GetWindowTextW(hwnd, wide, len + 1) : 0;
    char* utf8 = ng_windows_wide_to_utf8(wide, len);
    free(wide);
    return utf8;
}

int ng_windows_set_window_text(HWND hwnd, const char* utf8) {
    if (!IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    wchar_t* wide = ng_windows_utf8_to_wide(utf8);
    if (!wide) return NG_ERROR_INVALID_PARAMETER;
    BOOL ok = SetWindowTextW(hwnd, wide);
    free(wide);
    return ok ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_windows_tab_focus(HWND root, HWND focus, int back, int ctrl) {
    if (!root || !focus || focus == root || !IsChild(root, focus)) return 0;
    LRESULT code = SendMessageW(focus, WM_GETDLGCODE, VK_TAB, 0);
    int read_only = (GetWindowLongPtrW(focus, GWL_STYLE) & ES_READONLY) && (code & DLGC_HASSETSEL);
    if ((code & (DLGC_WANTTAB | DLGC_WANTALLKEYS)) && !ctrl && !read_only) return 0;
    HWND next = GetNextDlgTabItem(root, focus, back ? TRUE : FALSE);
    if (!next || next == focus) return 0;
    SetFocus(next);
    if ((SendMessageW(next, WM_GETDLGCODE, 0, 0) & DLGC_HASSETSEL) &&
        !(GetWindowLongPtrW(next, GWL_STYLE) & ES_MULTILINE)) {
        SendMessageW(next, EM_SETSEL, 0, -1);
    }
    return 1;
}

int ng_windows_enter_clicks(HWND focus) {
    if (!focus || !(SendMessageW(focus, WM_GETDLGCODE, 0, 0) & (DLGC_BUTTON | DLGC_DEFPUSHBUTTON | DLGC_UNDEFPUSHBUTTON))) return 0;
    LONG_PTR type = GetWindowLongPtrW(focus, GWL_STYLE) & BS_TYPEMASK;
    if (type != BS_PUSHBUTTON && type != BS_DEFPUSHBUTTON) return 0;
    SendMessageW(focus, BM_CLICK, 0, 0);
    return 1;
}

char* ng_windows_escape_mnemonics(const char* utf8) {
    size_t len = strlen(utf8), amps = 0;
    for (size_t i = 0; i < len; i++) amps += utf8[i] == '&';
    char* out = (char*)malloc(len + amps + 1);
    if (!out) return NULL;
    char* at = out;
    for (size_t i = 0; i < len; i++) {
        *at++ = utf8[i];
        if (utf8[i] == '&') *at++ = '&';
    }
    *at = 0;
    return out;
}

const wchar_t* ng_windows_get_class_name(void) {
    return CLASS_NAME;
}

/* Lifecycle code 8 is MemoryWarning in the Rust event_from_id table. */
void ng_windows_send_memory_warning(void) {
    for (NGWindow* w = ng_windows_windows(); w; w = w->next) {
        if (w->lifecycle) ng_invoke_lifecycle_callback((void*)w->hwnd, 8);
    }
}

void ng_windows_register_scale_callback(HWND hwnd, ScaleFactorCallback callback) {
    NGWindow* w = ng_windows_window_state(hwnd);
    if (w) w->on_scale = callback;
}

void ng_windows_register_lifecycle_callback(HWND hwnd) {
    NGWindow* w = ng_windows_window_state(hwnd);
    if (!w) return;
    w->lifecycle = TRUE;
    /* other windows moving over this one send it nothing, so look twice a second */
    SetTimer(hwnd, AUREA_OCCLUSION_TIMER_ID, 500, NULL);
    ng_windows_check_occlusion(hwnd);
}

unsigned int ng_windows_window_refresh_hz(void) {
    DWORD best = 0;
    for (NGWindow* w = ng_windows_windows(); w; w = w->next) {
        HMONITOR monitor = MonitorFromWindow(w->hwnd, MONITOR_DEFAULTTONEAREST);
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
    for (NGWindow* w = ng_windows_windows(); w; w = w->next) {
        RedrawWindow(w->hwnd, NULL, NULL, RDW_UPDATENOW | RDW_ALLCHILDREN);
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

double ng_windows_logical(HWND hwnd, int physical) {
    float scale = ng_windows_get_scale_factor((NGHandle)hwnd);
    return (double)physical / (scale > 1.0f ? (double)scale : 1.0);
}

/* Windows marks the mouse messages it makes from a pen or a touch with this signature. */
static unsigned int ng_windows_pointer_bits(void) {
    DWORD extra = (DWORD)GetMessageExtraInfo();
    if ((extra & 0xFFFFFF00u) != 0xFF515700u) return 0;
    if (extra & 0x80u) return ng_windows_touch_bits();
    return ng_windows_pen_bits();
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
        ng_windows_modifiers() | ng_windows_pointer_bits(),
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

#define AUREA_HOVER_TIMER_ID 0xA0E1
#define AUREA_ANY_BUTTON (MK_LBUTTON | MK_RBUTTON | MK_MBUTTON | MK_XBUTTON1 | MK_XBUTTON2)

static void ng_windows_track_leave(HWND target) {
    TRACKMOUSEEVENT tme = {0};
    tme.cbSize = sizeof(TRACKMOUSEEVENT);
    tme.dwFlags = TME_LEAVE;
    tme.hwndTrack = target;
    TrackMouseEvent(&tme);
}

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

static void ng_windows_arm_tracking(NGWindow* w) {
    HWND root = w->hwnd;
    if (w->tracking_leave) return;
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
    w->tracking_leave = TRUE;
    ng_windows_track_leave(target);
}

static void ng_windows_recheck_pointer(NGWindow* w) {
    HWND root = w->hwnd;
    if (!w->pointer_inside || GetCapture() == root) return;

    HWND under;
    if (ng_windows_pointer_in_content(root, &under)) {
        if (!w->tracking_leave) {
            SetTimer(root, AUREA_HOVER_TIMER_ID, 50, NULL);
        }
        return;
    }

    KillTimer(root, AUREA_HOVER_TIMER_ID);
    w->pointer_inside = FALSE;
    ng_invoke_cursor_entered((void*)root, 0);
}

/* The button a mouse button message is about, 0 to 4, whether it went down and its click count;
   -1 for any other message. */
static int ng_windows_button_of(UINT msg, WPARAM wParam, int* pressed, int* clicks) {
    int button;
    switch (msg) {
        case WM_LBUTTONDOWN: case WM_LBUTTONDBLCLK: case WM_LBUTTONUP: button = 0; break;
        case WM_RBUTTONDOWN: case WM_RBUTTONDBLCLK: case WM_RBUTTONUP: button = 1; break;
        case WM_MBUTTONDOWN: case WM_MBUTTONDBLCLK: case WM_MBUTTONUP: button = 2; break;
        case WM_XBUTTONDOWN: case WM_XBUTTONDBLCLK: case WM_XBUTTONUP:
            button = GET_XBUTTON_WPARAM(wParam) == XBUTTON1 ? 3 : 4;
            break;
        default: return -1;
    }
    *pressed = msg != WM_LBUTTONUP && msg != WM_RBUTTONUP && msg != WM_MBUTTONUP && msg != WM_XBUTTONUP;
    *clicks = (msg == WM_LBUTTONDBLCLK || msg == WM_RBUTTONDBLCLK || msg == WM_MBUTTONDBLCLK
               || msg == WM_XBUTTONDBLCLK) ? 2 : 1;
    return button;
}

static void ng_windows_release_capture(HWND hwnd, WPARAM held) {
    if (!(held & AUREA_ANY_BUTTON) && GetCapture() == hwnd) {
        ReleaseCapture();
    }
}

LRESULT CALLBACK WindowProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam) {
    if (uMsg == WM_NCCREATE && !ng_windows_window_state_create(hwnd)) return FALSE;
    NGWindow* w = ng_windows_window_state(hwnd);
    if (!w) return DefWindowProcW(hwnd, uMsg, wParam, lParam);
    if (uMsg == WM_NCDESTROY) {
        ng_windows_window_state_free(hwnd);
        if (!ng_windows_windows()) PostQuitMessage(0);
        return DefWindowProcW(hwnd, uMsg, wParam, lParam);
    }

    LRESULT ime_result;
    if (ng_windows_ime_message(hwnd, uMsg, wParam, lParam, &ime_result)) {
        return ime_result;
    }
    ng_windows_wintab_message(hwnd, uMsg, lParam);

    int pressed, clicks;
    int button = ng_windows_button_of(uMsg, wParam, &pressed, &clicks);
    if (button >= 0) {
        if (pressed) {
            SetCapture(hwnd);
            /* Before the press goes out, so the app has the text when it hears the click. */
            ng_windows_ime_complete(hwnd);
        }
        ng_windows_emit_mouse_button(hwnd, lParam, button, pressed, clicks);
        if (!pressed) ng_windows_release_capture(hwnd, wParam);
        return DefWindowProcW(hwnd, uMsg, wParam, lParam);
    }

    switch (uMsg) {
        case WM_SETFOCUS:
            ng_invoke_focus_changed((void*)hwnd, 1);
            break;
        case WM_ACTIVATE:
            ng_windows_cursor_grab_activate((void*)hwnd, LOWORD(wParam) != WA_INACTIVE);
            ng_windows_wintab_activate(hwnd, LOWORD(wParam) != WA_INACTIVE);
            break;
        case WM_KILLFOCUS:
            w->high_surrogate = 0;
            ng_invoke_focus_changed((void*)hwnd, 0);
            break;
        case WM_MOUSEMOVE:
            if (!w->pointer_inside) {
                w->pointer_inside = TRUE;
                ng_invoke_cursor_entered((void*)hwnd, 1);
            }
            ng_windows_arm_tracking(w);
            ng_invoke_mouse_move(
                (void*)hwnd,
                ng_windows_logical(hwnd, GET_X_LPARAM(lParam)),
                ng_windows_logical(hwnd, GET_Y_LPARAM(lParam)),
                ng_windows_buttons(wParam),
                ng_windows_modifiers() | ng_windows_pointer_bits());
            break;
        case WM_POINTERDOWN:
        case WM_POINTERUPDATE:
        case WM_POINTERUP:
            ng_windows_pen_message(hwnd, uMsg, wParam);
            break;
        case WM_POINTERCAPTURECHANGED:
            ng_windows_pen_capture_lost(hwnd, wParam);
            break;
        case WM_MOUSELEAVE:
            w->tracking_leave = FALSE;
            ng_windows_recheck_pointer(w);
            break;
        case WM_CAPTURECHANGED:
            if ((HWND)lParam != hwnd) ng_windows_recheck_pointer(w);
            break;
        case WM_SETCURSOR:
            if (LOWORD(lParam) == HTCLIENT && (HWND)wParam == hwnd) {
                SetCursor((HCURSOR)ng_windows_window_cursor((NGHandle)hwnd));
                return TRUE;
            }
            break;
        case WM_TIMER:
            if (wParam == AUREA_OCCLUSION_TIMER_ID) {
                ng_windows_check_occlusion(hwnd);
                return 0;
            }
            if (wParam == AUREA_HOVER_TIMER_ID) {
                ng_windows_recheck_pointer(w);
                if (w->pointer_inside) ng_windows_arm_tracking(w);
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
        case WM_INPUT:
            ng_windows_raw_input(hwnd, lParam);
            break;
        case WM_KEYDOWN:
        case WM_SYSKEYDOWN: {
            unsigned int keycode = ng_windows_keycode_from_vk(wParam);
            unsigned int modifiers = ng_windows_modifiers();
            int menu = ng_windows_menu_key_down(hwnd, uMsg, wParam, keycode, modifiers);
            if (menu == NG_MENU_KEY_DEFAULT) break;
            if (menu == NG_MENU_KEY_TAKEN) return 0;
            ng_invoke_key_event((void*)hwnd, keycode, 1, modifiers);
            if (wParam == VK_MENU || wParam == VK_F10) {
                return 0;
            }
            break;
        }
        case WM_KEYUP:
        case WM_SYSKEYUP: {
            int menu = ng_windows_menu_key_up(hwnd, uMsg, wParam);
            if (menu == NG_MENU_KEY_DEFAULT) break;
            if (menu == NG_MENU_KEY_TAKEN) return 0;
            unsigned int keycode = ng_windows_keycode_from_vk(wParam);
            ng_invoke_key_event((void*)hwnd, keycode, 0, ng_windows_modifiers());
            if (wParam == VK_MENU || wParam == VK_F10) {
                return 0;
            }
            break;
        }
        case WM_SYSCHAR:
            if (w->shortcut_took_syskey) {
                w->shortcut_took_syskey = FALSE;
                return 0;
            }
            break;
        case WM_CHAR:
            ng_windows_text_char(w, (wchar_t)wParam);
            break;
        case WM_MOUSEWHEEL: {
            double delta = (double)GET_WHEEL_DELTA_WPARAM(wParam) / (double)WHEEL_DELTA;
            /* Shift turns the wheel sideways, as Windows apps expect; down goes right, like a tilt. */
            double cx = 0.0, cy = delta;
            if (GET_KEYSTATE_WPARAM(wParam) & MK_SHIFT) {
                cx = -delta;
                cy = 0.0;
            }
            /* wheel clicks, fractional from fine wheels; rust turns them into pixels */
            ng_invoke_scroll((void*)hwnd, 0.0, 0.0, 1, cx, cy, NG_SCROLL_WHEEL, NG_SCROLL_DISCRETE, 0,
                             ng_windows_modifiers());
            break;
        }
        case WM_MOUSEHWHEEL: {
            double delta = (double)GET_WHEEL_DELTA_WPARAM(wParam) / (double)WHEEL_DELTA;
            ng_invoke_scroll((void*)hwnd, 0.0, 0.0, 1, delta, 0.0, NG_SCROLL_WHEEL, NG_SCROLL_DISCRETE, 0,
                             ng_windows_modifiers());
            break;
        }
        case WM_GESTURE:
            if (ng_windows_gesture_message(hwnd, lParam)) return 0;
            break;
        case WM_HSCROLL:
        case WM_VSCROLL:
            ng_windows_control_changed(uMsg, wParam, lParam);
            break;
        case WM_COMMAND:
            if (ng_windows_control_changed(uMsg, wParam, lParam)) {
                break;
            } else if (HIWORD(wParam) == 0) {
                unsigned int command_id = LOWORD(wParam);
                if (ng_windows_button_command(command_id)) break;
                ng_windows_menu_command(command_id, lParam);
            }
            break;
        case WM_SETTINGCHANGE:
            if (lParam && lstrcmpiW((LPCWSTR)lParam, L"ImmersiveColorSet") == 0) {
                ng_windows_theme_check();
            }
            break;
        case WM_DPICHANGED: {
            if (w->on_scale) w->on_scale((void*)hwnd, (float)LOWORD(wParam) / 96.0f);
            RECT* suggested = (RECT*)lParam;
            SetWindowPos(hwnd, NULL,
                suggested->left, suggested->top,
                suggested->right - suggested->left,
                suggested->bottom - suggested->top,
                SWP_NOZORDER | SWP_NOACTIVATE);
            ng_windows_refresh_dpi(hwnd);
            return 0;
        }
        case WM_CLOSE:
            ng_windows_lifecycle(hwnd, 5);
            if (w->close_guard) return 0;
            if (w->hide_on_close) {
                ShowWindow(hwnd, SW_HIDE);
                return 0;
            }
            break;
        case WM_GETMINMAXINFO: {
            if (w->min_client_w <= 0 && w->min_client_h <= 0) break;
            RECT outer, client;
            if (!GetWindowRect(hwnd, &outer) || !GetClientRect(hwnd, &client)) break;
            MINMAXINFO* info = (MINMAXINFO*)lParam;
            /* Floored like a size set, so the minimum reads back as itself. */
            long long dpi = ng_windows_dpi(hwnd);
            if (w->min_client_w > 0) {
                info->ptMinTrackSize.x = (LONG)(w->min_client_w * dpi / 96) + (outer.right - outer.left) - client.right;
            }
            if (w->min_client_h > 0) {
                info->ptMinTrackSize.y = (LONG)(w->min_client_h * dpi / 96) + (outer.bottom - outer.top) - client.bottom;
            }
            return 0;
        }
        case WM_SIZE:
            ng_windows_cursor_grab_refresh((void*)hwnd);
            if (wParam == SIZE_MINIMIZED) {
                if (w->minimized) break;
                w->minimized = TRUE;
                ng_windows_lifecycle(hwnd, 6);
                ng_windows_lifecycle(hwnd, 9);
            } else {
                ng_windows_fit_content(hwnd);
                if (w->minimized) {
                    w->minimized = FALSE;
                    ng_windows_lifecycle(hwnd, 7);
                    ng_windows_lifecycle(hwnd, 10);
                }
                ng_windows_lifecycle(hwnd, 12);
            }
            break;
        case WM_WINDOWPOSCHANGED:
            ng_windows_check_occlusion(hwnd);
            break;
        case WM_MOVE: {
            ng_windows_cursor_grab_refresh((void*)hwnd);
            HMONITOR monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if (w->monitor != monitor) {
                w->monitor = monitor;
                ng_windows_display_changed();
            }
            ng_windows_lifecycle(hwnd, 11);
            break;
        }
        case WM_DESTROY:
            ng_windows_wintab_destroyed(hwnd);
            ng_windows_release_window_input((void*)hwnd);
            ng_windows_refuse_drops(hwnd);
            KillTimer(hwnd, AUREA_OCCLUSION_TIMER_ID);
            KillTimer(hwnd, AUREA_HOVER_TIMER_ID);
            ng_shortcuts_clear((void*)hwnd);
            return 0;
        default:
            break;
    }
    return DefWindowProcW(hwnd, uMsg, wParam, lParam);
}

void ng_windows_beep(void) {
    MessageBeep(MB_OK);
}

int ng_windows_prefers_dark(void) {
    HKEY key = NULL;
    if (RegOpenKeyExW(HKEY_CURRENT_USER, L"Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
                      0, KEY_QUERY_VALUE, &key) != ERROR_SUCCESS) {
        return 0;
    }
    DWORD light = 1;
    DWORD size = sizeof(light);
    DWORD type = 0;
    LONG status = RegQueryValueExW(key, L"AppsUseLightTheme", NULL, &type, (LPBYTE)&light, &size);
    RegCloseKey(key);
    if (status != ERROR_SUCCESS || type != REG_DWORD) return 0;
    return light ? 0 : 1;
}

static void ng_windows_theme_check(void) {
    int dark = ng_windows_prefers_dark();
    if (dark == g_theme_dark) return;
    BOOL known = g_theme_dark >= 0;
    g_theme_dark = dark;
    if (known) ng_invoke_theme_changed(dark);
}

int ng_windows_wheel_lines(void) {
    UINT lines = 3;
    if (!SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &lines, 0)) return 3;
    return lines == WHEEL_PAGESCROLL || lines > 1000 ? -1 : (int)lines;
}

int ng_windows_reduced_motion(void) {
    BOOL animate = TRUE;
    if (!SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &animate, 0)) return 0;
    return animate ? 0 : 1;
}

/* XP has no preferred-language list: the user's locale as `lang-REGION`. */
static int ng_windows_user_locale(WCHAR* out, int cap) {
    WCHAR lang[16], region[16];
    if (!GetLocaleInfoW(LOCALE_USER_DEFAULT, LOCALE_SISO639LANGNAME, lang, 16)) return 0;
    if (!GetLocaleInfoW(LOCALE_USER_DEFAULT, LOCALE_SISO3166CTRYNAME, region, 16)) region[0] = 0;
    int n = region[0] ? _snwprintf_s(out, cap, _TRUNCATE, L"%s-%s", lang, region)
                      : _snwprintf_s(out, cap, _TRUNCATE, L"%s", lang);
    if (n < 0 || n + 2 > cap) return 0;
    out[n + 1] = 0;
    return n + 2;
}

int ng_windows_locales(char* buf, int cap) {
    typedef BOOL (WINAPI *PreferredFunc)(DWORD, PULONG, PZZWSTR, PULONG);
    PreferredFunc preferred = (PreferredFunc)ng_windows_api("kernel32.dll", "GetUserPreferredUILanguages");
    ULONG count = 0;
    ULONG len = 0;
    WCHAR fallback[40];
    WCHAR* wide = NULL;
    if (preferred) {
        if (!preferred(MUI_LANGUAGE_NAME, &count, NULL, &len) || len < 2) return 0;
        wide = (WCHAR*)malloc(len * sizeof(WCHAR));
        if (!wide) return 0;
    } else {
        len = (ULONG)ng_windows_user_locale(fallback, 40);
        if (len < 2) return 0;
    }
    int need = 0;
    if (!preferred || (preferred(MUI_LANGUAGE_NAME, &count, wide, &len) && len >= 2)) {
        if (!preferred) wide = fallback;
        int chars = (int)len - 1;
        need = WideCharToMultiByte(CP_UTF8, 0, wide, chars, NULL, 0, NULL, NULL);
        if (buf && need > 0 && cap >= need) {
            WideCharToMultiByte(CP_UTF8, 0, wide, chars, buf, cap, NULL, NULL);
        }
    }
    if (preferred) free(wide);
    return need;
}

void ng_windows_fit_content(HWND window) {
    NGWindow* state = ng_windows_window_state(window);
    HWND content = state ? state->content : NULL;
    if (!content || !IsWindow(content)) return;
    RECT client;
    GetClientRect(window, &client);
    int w = client.right - client.left;
    int h = client.bottom - client.top;
    if (w <= 0 || h <= 0) return;
    SetWindowPos(content, NULL, 0, 0, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    if (GetPropA(content, BOX_ORIENTATION_PROP)) {
        layout_box_children(content);
    }
}
