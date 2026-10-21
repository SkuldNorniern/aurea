
#include "tray.h"
#include "utils.h"
#include "window.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <windowsx.h>
#include <shellapi.h>
#include <stdlib.h>
#include <string.h>

#define TRAY_MESSAGE (WM_APP + 0x41)
#define TRAY_MAX_ITEMS 64

typedef struct {
    HWND hwnd;
    UINT uid;
    HICON icon;
    unsigned int click_id;
    unsigned int item_ids[TRAY_MAX_ITEMS];
    wchar_t* item_titles[TRAY_MAX_ITEMS];
    int item_count;
    wchar_t tip[128];
    DWORD size;
    int modern;
} Tray;

static const wchar_t* TRAY_CLASS = L"AureaTrayWindow";
static UINT g_taskbar_created = 0;
static UINT g_next_uid = 1;

static void tray_fill(Tray* tray, NOTIFYICONDATAW* data, UINT flags) {
    memset(data, 0, sizeof(*data));
    data->cbSize = tray->size ? tray->size : sizeof(*data);
    if (data->cbSize < sizeof(*data)) flags &= ~(UINT)NIF_SHOWTIP;
    data->hWnd = tray->hwnd;
    data->uID = tray->uid;
    data->uFlags = flags;
    data->uCallbackMessage = TRAY_MESSAGE;
    data->hIcon = tray->icon ? tray->icon : LoadIcon(NULL, IDI_APPLICATION);
    wcsncpy(data->szTip, tray->tip, sizeof(data->szTip) / sizeof(wchar_t) - 1);
}

static int tray_add(Tray* tray) {
    NOTIFYICONDATAW data;
    tray_fill(tray, &data, NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP);
    if (!Shell_NotifyIconW(NIM_ADD, &data)) {
        tray->size = NOTIFYICONDATAW_V3_SIZE;
        tray_fill(tray, &data, NIF_MESSAGE | NIF_ICON | NIF_TIP);
        if (!Shell_NotifyIconW(NIM_ADD, &data)) return 0;
    }
    data.uVersion = NOTIFYICON_VERSION_4;
    tray->modern = Shell_NotifyIconW(NIM_SETVERSION, &data) ? 1 : 0;
    return 1;
}

static void tray_show_menu(Tray* tray, int x, int y) {
    if (tray->item_count == 0) return;
    HMENU menu = CreatePopupMenu();
    if (!menu) return;
    for (int i = 0; i < tray->item_count; i++) {
        if (tray->item_titles[i]) {
            AppendMenuW(menu, MF_STRING, (UINT_PTR)(i + 1), tray->item_titles[i]);
        } else {
            AppendMenuW(menu, MF_SEPARATOR, 0, NULL);
        }
    }
    SetForegroundWindow(tray->hwnd);
    UINT chosen = (UINT)TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
        x, y, 0, tray->hwnd, NULL);
    PostMessageW(tray->hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    if (chosen >= 1 && (int)chosen <= tray->item_count) {
        ng_invoke_menu_callback(tray->item_ids[chosen - 1]);
    }
}

static LRESULT CALLBACK tray_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    Tray* tray = (Tray*)GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if (tray && msg == TRAY_MESSAGE && !tray->modern) {
        POINT at;
        if (LOWORD(lParam) == WM_LBUTTONUP) {
            ng_invoke_menu_callback(tray->click_id);
        } else if (LOWORD(lParam) == WM_RBUTTONUP && GetCursorPos(&at)) {
            tray_show_menu(tray, at.x, at.y);
        }
        return 0;
    }
    if (tray && msg == TRAY_MESSAGE) {
        switch (LOWORD(lParam)) {
            case NIN_SELECT:
            case NIN_KEYSELECT:
                ng_invoke_menu_callback(tray->click_id);
                return 0;
            case WM_CONTEXTMENU:
                tray_show_menu(tray, GET_X_LPARAM(wParam), GET_Y_LPARAM(wParam));
                return 0;
            default:
                return 0;
        }
    }
    if (tray && g_taskbar_created && msg == g_taskbar_created) {
        tray_add(tray);
        return 0;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

static int tray_register_class(void) {
    static int registered = 0;
    if (registered) return 1;
    WNDCLASSW wc = {0};
    wc.lpfnWndProc = tray_proc;
    wc.hInstance = GetModuleHandleW(NULL);
    wc.lpszClassName = TRAY_CLASS;
    if (!RegisterClassW(&wc) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS) {
        return 0;
    }
    g_taskbar_created = RegisterWindowMessageW(L"TaskbarCreated");
    registered = 1;
    return 1;
}

static void tray_set_tip(Tray* tray, const char* tooltip) {
    tray->tip[0] = L'\0';
    if (!tooltip) return;
    wchar_t* wide = ng_windows_utf8_to_wide(tooltip);
    if (!wide) return;
    wcsncpy(tray->tip, wide, sizeof(tray->tip) / sizeof(wchar_t) - 1);
    tray->tip[sizeof(tray->tip) / sizeof(wchar_t) - 1] = L'\0';
    free(wide);
}

NGHandle ng_windows_tray_create(const char* tooltip, unsigned int click_id) {
    if (!tray_register_class()) return NULL;
    Tray* tray = (Tray*)calloc(1, sizeof(Tray));
    if (!tray) return NULL;
    tray->hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW, TRAY_CLASS, L"", WS_POPUP,
        0, 0, 0, 0, NULL, NULL, GetModuleHandleW(NULL), NULL);
    if (!tray->hwnd) {
        free(tray);
        return NULL;
    }
    SetWindowLongPtrW(tray->hwnd, GWLP_USERDATA, (LONG_PTR)tray);
    tray->uid = g_next_uid++;
    tray->click_id = click_id;
    tray_set_tip(tray, tooltip);
    if (!tray_add(tray)) {
        DestroyWindow(tray->hwnd);
        free(tray);
        return NULL;
    }
    return (NGHandle)tray;
}

void ng_windows_tray_destroy(NGHandle handle) {
    Tray* tray = (Tray*)handle;
    if (!tray) return;
    NOTIFYICONDATAW data;
    tray_fill(tray, &data, 0);
    Shell_NotifyIconW(NIM_DELETE, &data);
    SetWindowLongPtrW(tray->hwnd, GWLP_USERDATA, 0);
    DestroyWindow(tray->hwnd);
    if (tray->icon) DestroyIcon(tray->icon);
    for (int i = 0; i < tray->item_count; i++) {
        free(tray->item_titles[i]);
    }
    free(tray);
}

int ng_windows_tray_set_icon_rgba(
    NGHandle handle,
    const unsigned char* rgba,
    unsigned int width,
    unsigned int height
) {
    Tray* tray = (Tray*)handle;
    if (!tray) return NG_ERROR_INVALID_PARAMETER;
    HICON icon = ng_windows_icon_from_rgba(rgba, width, height);
    if (!icon) return NG_ERROR_PLATFORM_SPECIFIC;
    HICON previous = tray->icon;
    tray->icon = icon;
    NOTIFYICONDATAW data;
    tray_fill(tray, &data, NIF_ICON);
    int ok = Shell_NotifyIconW(NIM_MODIFY, &data);
    if (previous) DestroyIcon(previous);
    return ok ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_windows_tray_set_tooltip(NGHandle handle, const char* tooltip) {
    Tray* tray = (Tray*)handle;
    if (!tray) return NG_ERROR_INVALID_PARAMETER;
    tray_set_tip(tray, tooltip);
    NOTIFYICONDATAW data;
    tray_fill(tray, &data, NIF_TIP | NIF_SHOWTIP);
    return Shell_NotifyIconW(NIM_MODIFY, &data) ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_windows_tray_add_item(NGHandle handle, const char* title, unsigned int id) {
    Tray* tray = (Tray*)handle;
    if (!tray) return NG_ERROR_INVALID_PARAMETER;
    if (tray->item_count >= TRAY_MAX_ITEMS) return NG_ERROR_PLATFORM_SPECIFIC;
    wchar_t* wide = NULL;
    if (title) {
        char* shown = ng_windows_escape_mnemonics(title);
        wide = shown ? ng_windows_utf8_to_wide(shown) : NULL;
        free(shown);
        if (!wide) return NG_ERROR_INVALID_PARAMETER;
    }
    tray->item_titles[tray->item_count] = wide;
    tray->item_ids[tray->item_count] = id;
    tray->item_count++;
    return NG_SUCCESS;
}

int ng_windows_tray_notify(NGHandle handle, const char* title, const char* body) {
    Tray* tray = (Tray*)handle;
    if (!tray) return NG_ERROR_INVALID_PARAMETER;
    NOTIFYICONDATAW data;
    tray_fill(tray, &data, NIF_INFO);
    data.dwInfoFlags = data.cbSize < sizeof(data) ? NIIF_INFO : NIIF_USER | NIIF_LARGE_ICON;
    if (tray->icon && data.cbSize >= sizeof(data)) data.hBalloonIcon = tray->icon;
    wchar_t* wide_title = ng_windows_utf8_to_wide(title ? title : "");
    wchar_t* wide_body = ng_windows_utf8_to_wide(body ? body : "");
    if (wide_title) {
        wcsncpy(data.szInfoTitle, wide_title, sizeof(data.szInfoTitle) / sizeof(wchar_t) - 1);
        free(wide_title);
    }
    if (wide_body) {
        wcsncpy(data.szInfo, wide_body, sizeof(data.szInfo) / sizeof(wchar_t) - 1);
        free(wide_body);
    }
    return Shell_NotifyIconW(NIM_MODIFY, &data) ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}
