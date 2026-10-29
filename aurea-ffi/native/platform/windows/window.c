#include "window.h"
#include "occlusion.h"
#include "drop.h"
#include "gesture.h"
#include "common/clipboard.h"
#include "elements/common.h"
#include "utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <ShellScalingApi.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>

static const char* AUREA_WINDOW_ICON_PROPERTY = "AureaWindowIcon";

#define LOG_ERROR(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_error(buf); \
} while(0)

#define LOG_WARN(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_warn(buf); \
} while(0)

#define LOG_INFO(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_info(buf); \
} while(0)

#define LOG_DEBUG(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_debug(buf); \
} while(0)

#define LOG_TRACE(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_trace(buf); \
} while(0)

#define AUREA_CURSOR_GRAB_PROP "AureaCursorGrabMode"

static void ng_windows_set_client_size(HWND hwnd, int width, int height, UINT flags) {
    RECT outer;
    RECT client;
    if (!GetWindowRect(hwnd, &outer) || !GetClientRect(hwnd, &client)) return;
    int frame_w = (outer.right - outer.left) - (client.right - client.left);
    int frame_h = (outer.bottom - outer.top) - (client.bottom - client.top);
    int dpi = (int)ng_windows_dpi(hwnd);
    if (dpi <= 0) dpi = 96;
    int w = (int)(((long long)width * dpi) / 96);
    int h = (int)(((long long)height * dpi) / 96);
    SetWindowPos(hwnd, NULL, 0, 0, w + frame_w, h + frame_h,
        flags | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
}

static int ng_windows_unscale(HWND hwnd, int device) {
    long long dpi = (long long)ng_windows_dpi(hwnd);
    if (dpi <= 0) dpi = 96;
    if (device <= 0) return 0;
    return (int)(((long long)device * 96 + dpi - 1) / dpi);
}
/* A window asked bigger than its monitor's work area would hide its own edges. */
static void ng_windows_fit_work_area(HWND hwnd) {
    MONITORINFO info = {sizeof(info)};
    RECT r;
    if (!GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &info) ||
        !GetWindowRect(hwnd, &r)) {
        return;
    }
    RECT work = info.rcWork;
    int w = r.right - r.left, h = r.bottom - r.top;
    if (w > work.right - work.left) w = work.right - work.left;
    if (h > work.bottom - work.top) h = work.bottom - work.top;
    int x = r.left, y = r.top;
    if (x + w > work.right) x = work.right - w;
    if (y + h > work.bottom) y = work.bottom - h;
    if (x < work.left) x = work.left;
    if (y < work.top) y = work.top;
    if (x != r.left || y != r.top || w != r.right - r.left || h != r.bottom - r.top) {
        SetWindowPos(hwnd, NULL, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

NGHandle ng_windows_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    
    wchar_t* wide_title = ng_windows_utf8_to_wide(title);
    if (!wide_title) return NULL;

    HWND hwnd = CreateWindowExW(
        0,
        ng_windows_get_class_name(),
        wide_title,
        WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT, CW_USEDEFAULT,
        width, height,
        NULL,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    free(wide_title);
    
    if (hwnd) {
        ng_windows_set_client_size(hwnd, width, height, 0);
        ng_windows_fit_work_area(hwnd);
        ng_windows_accept_drops(hwnd);
        ng_windows_enable_gestures(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetFocus(hwnd);
    }

    return (NGHandle)hwnd;
}

#define AUREA_WINDOW_NORMAL  0
#define AUREA_WINDOW_POPUP   1
#define AUREA_WINDOW_TOOL    2
#define AUREA_WINDOW_UTILITY 3
#define AUREA_WINDOW_SHEET   4
#define AUREA_WINDOW_DIALOG  5

static int window_styles_for(int window_type, DWORD* style, DWORD* ex_style) {
    switch (window_type) {
        case AUREA_WINDOW_NORMAL:
            *style = WS_OVERLAPPEDWINDOW;
            *ex_style = 0;
            return 1;
        case AUREA_WINDOW_POPUP:
            *style = WS_POPUP;
            *ex_style = WS_EX_TOPMOST;
            return 1;
        case AUREA_WINDOW_TOOL:
        case AUREA_WINDOW_UTILITY:
            *style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME;
            *ex_style = WS_EX_TOOLWINDOW;
            return 1;
        case AUREA_WINDOW_DIALOG:
            *style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
            *ex_style = WS_EX_DLGMODALFRAME;
            return 1;
        default:
            return 0;
    }
}

NGHandle ng_windows_create_window_with_type(const char* title, int width, int height, int window_type) {
    if (!title) return NULL;

    DWORD style = 0;
    DWORD ex_style = 0;
    if (!window_styles_for(window_type, &style, &ex_style)) {
        return NULL;
    }

    wchar_t* wide_title = ng_windows_utf8_to_wide(title);
    if (!wide_title) return NULL;

    HWND hwnd = CreateWindowExW(
        ex_style,
        ng_windows_get_class_name(),
        wide_title,
        style,
        CW_USEDEFAULT, CW_USEDEFAULT,
        width, height,
        NULL,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    free(wide_title);

    if (hwnd) {
        ng_windows_set_client_size(hwnd, width, height, 0);
        ng_windows_fit_work_area(hwnd);
        ng_windows_accept_drops(hwnd);
        ng_windows_enable_gestures(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetFocus(hwnd);
    }

    return (NGHandle)hwnd;
}

float ng_windows_get_scale_factor(NGHandle window) {
    if (!window) return 1.0f;
    HWND hwnd = (HWND)window;
    
    typedef UINT (WINAPI *GetDpiForWindowFunc)(HWND);
    static GetDpiForWindowFunc getDpiForWindow = NULL;
    static BOOL looked_up = FALSE;
    if (!looked_up) {
        HMODULE user32 = GetModuleHandleA("user32.dll");
        if (user32) {
            getDpiForWindow = (GetDpiForWindowFunc)GetProcAddress(user32, "GetDpiForWindow");
        }
        looked_up = TRUE;
    }
    if (getDpiForWindow) {
        UINT dpi = getDpiForWindow(hwnd);
        if (dpi > 0) {
            return (float)dpi / 96.0f;
        }
    }
    
    HMONITOR monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    if (monitor) {
        UINT dpiX = 96;
        UINT dpiY = 96;
        typedef HRESULT (WINAPI *MonitorDpiFunc)(HMONITOR, int, UINT*, UINT*);
        MonitorDpiFunc monitor_dpi = (MonitorDpiFunc)ng_windows_api("shcore.dll", "GetDpiForMonitor");
        if (monitor_dpi && monitor_dpi(monitor, MDT_EFFECTIVE_DPI, &dpiX, &dpiY) == S_OK) {
            return (float)dpiX / 96.0f;
        }
    }
    return 1.0f;
}

void ng_windows_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    if (!window) return;
    HWND hwnd = (HWND)window;
    
    typedef BOOL (WINAPI *EnableNonClientDpiScalingFunc)(HWND);
    HMODULE user32 = GetModuleHandleA("user32.dll");
    if (user32) {
        EnableNonClientDpiScalingFunc enableNonClientDpiScaling = 
            (EnableNonClientDpiScalingFunc)GetProcAddress(user32, "EnableNonClientDpiScaling");
        if (enableNonClientDpiScaling) {
            enableNonClientDpiScaling(hwnd);
        }
    }
    
    extern void ng_windows_register_scale_callback(HWND, ScaleFactorCallback);
    ng_windows_register_scale_callback(hwnd, callback);
}

void ng_windows_window_set_lifecycle_callback(NGHandle window) {
    if (!window) return;
    HWND hwnd = (HWND)window;
    extern void ng_windows_register_lifecycle_callback(HWND);
    ng_windows_register_lifecycle_callback(hwnd);
}

void ng_windows_destroy_window(NGHandle handle) {
    if (!handle) return;
    HWND hwnd = (HWND)handle;
    HICON icon = (HICON)RemovePropA(hwnd, AUREA_WINDOW_ICON_PROPERTY);
    SendMessageA(hwnd, WM_SETICON, ICON_BIG, 0);
    SendMessageA(hwnd, WM_SETICON, ICON_SMALL, 0);
    if (icon) DestroyIcon(icon);
    ng_windows_release_window_input(hwnd);
    DestroyWindow(hwnd);
}

void ng_windows_destroy_element(NGHandle element) {
    if (!element) return;
    HWND hwnd = (HWND)element;
    if (!IsWindow(hwnd)) return;
    DestroyWindow(hwnd);
}

int ng_windows_detach_element(NGHandle element) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    HWND hwnd = (HWND)element;
    if (!IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    ShowWindow(hwnd, SW_HIDE);
    if (!SetParent(hwnd, NULL)) return NG_ERROR_PLATFORM_SPECIFIC;
    return NG_SUCCESS;
}

#define AUREA_MAXIMIZE_ON_SHOW_PROP "AureaMaximizeOnShow"

/* How far workspace coordinates sit from screen ones on `monitor`. */
static POINT ng_windows_workspace_offset(HWND hwnd, HMONITOR monitor) {
    POINT offset = {0, 0};
    if (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW) return offset;
    MONITORINFO info = {sizeof info};
    if (GetMonitorInfoW(monitor, &info)) {
        offset.x = info.rcWork.left - info.rcMonitor.left;
        offset.y = info.rcWork.top - info.rcMonitor.top;
    }
    return offset;
}

int ng_windows_window_refresh_rate(NGHandle window) {
    HWND hwnd = (HWND)window;
    if (!hwnd) return 0;
    MONITORINFOEXW info;
    info.cbSize = sizeof info;
    if (!GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), (MONITORINFO*)&info)) return 0;
    DEVMODEW mode = {0};
    mode.dmSize = sizeof mode;
    if (!EnumDisplaySettingsW(info.szDevice, ENUM_CURRENT_SETTINGS, &mode)) return 0;
    /* 0 and 1 mean the hardware default */
    return mode.dmDisplayFrequency > 1 ? (int)mode.dmDisplayFrequency * 1000 : 0;
}

int ng_windows_window_request_attention(NGHandle window, int critical) {
    HWND hwnd = (HWND)window;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (GetForegroundWindow() == hwnd) return NG_SUCCESS;
    /* a few flashes, or the caption too until the window comes to the front when critical */
    FLASHWINFO flash = {sizeof flash, hwnd, critical ? FLASHW_ALL | FLASHW_TIMERNOFG : FLASHW_TRAY,
                        critical ? 0 : 3, 0};
    FlashWindowEx(&flash);
    return NG_SUCCESS;
}

int ng_windows_window_get_placement(NGHandle window, int* rect) {
    HWND hwnd = (HWND)window;
    if (!hwnd || !IsWindow(hwnd) || !rect) return NG_ERROR_INVALID_HANDLE;
    WINDOWPLACEMENT wp = {sizeof wp};
    if (!GetWindowPlacement(hwnd, &wp)) return NG_ERROR_PLATFORM_SPECIFIC;
    RECT r = wp.rcNormalPosition;
    /* the rect is relative to the work area of the monitor the window is on */
    POINT offset = ng_windows_workspace_offset(hwnd, MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST));
    rect[0] = r.left + offset.x;
    rect[1] = r.top + offset.y;
    rect[2] = r.right - r.left;
    rect[3] = r.bottom - r.top;
    BOOL minimized = IsIconic(hwnd);
    BOOL maximized = minimized ? (wp.flags & WPF_RESTORETOMAXIMIZED) != 0 : IsZoomed(hwnd);
    if (!IsWindowVisible(hwnd) && GetPropA(hwnd, AUREA_MAXIMIZE_ON_SHOW_PROP)) maximized = TRUE;
    return (maximized ? 1 : 0) | (minimized ? 2 : 0);
}

int ng_windows_window_set_placement(NGHandle window, const int* rect, int maximized) {
    HWND hwnd = (HWND)window;
    if (!hwnd || !IsWindow(hwnd) || !rect) return NG_ERROR_INVALID_HANDLE;
    if (rect[2] <= 0 || rect[3] <= 0) return NG_ERROR_INVALID_PARAMETER;
    RECT r = {rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]};
    POINT offset = ng_windows_workspace_offset(hwnd, MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST));
    OffsetRect(&r, -offset.x, -offset.y);
    BOOL visible = IsWindowVisible(hwnd);
    WINDOWPLACEMENT wp = {sizeof wp};
    GetWindowPlacement(hwnd, &wp);
    wp.flags = 0;
    wp.rcNormalPosition = r;
    /* a hidden window stays hidden and maximizes when shown */
    wp.showCmd = !visible ? SW_HIDE : maximized ? SW_SHOWMAXIMIZED : SW_SHOWNORMAL;
    /* Windows moves a rect that would be off every screen back onto one */
    if (!SetWindowPlacement(hwnd, &wp)) return NG_ERROR_PLATFORM_SPECIFIC;
    if (visible || !maximized) {
        RemovePropA(hwnd, AUREA_MAXIMIZE_ON_SHOW_PROP);
    } else {
        SetPropA(hwnd, AUREA_MAXIMIZE_ON_SHOW_PROP, (HANDLE)1);
    }
    return NG_SUCCESS;
}

void ng_windows_window_show(NGHandle window) {
    if (!window) return;
    HWND hwnd = (HWND)window;
    if (RemovePropA(hwnd, AUREA_MAXIMIZE_ON_SHOW_PROP)) {
        ShowWindow(hwnd, SW_SHOWMAXIMIZED);
    } else {
        ShowWindow(hwnd, IsIconic(hwnd) ? SW_RESTORE : SW_SHOW);
    }
    SetForegroundWindow(hwnd);
    UpdateWindow(hwnd);
}

void ng_windows_window_hide(NGHandle window) {
    if (!window) return;
    ShowWindow((HWND)window, SW_HIDE);
}

int ng_windows_window_is_visible(NGHandle window) {
    if (!window) return 0;
    return IsWindowVisible((HWND)window) ? 1 : 0;
}

int ng_windows_window_occlusion(NGHandle window) {
    return ng_windows_occlusion_of((HWND)window);
}

NGHandle ng_windows_window_get_content_view(NGHandle window) {
    return window;
}

int ng_windows_set_window_content(NGHandle window_handle, NGHandle content_handle) {
    LOG_INFO("ng_windows_set_window_content: called");
    
    if (!window_handle || !content_handle) {
        LOG_ERROR("ng_windows_set_window_content: Invalid handles");
        return NG_ERROR_INVALID_HANDLE;
    }

    HWND window = (HWND)window_handle;
    HWND content = (HWND)content_handle;

    SetParent(content, window);

    LONG_PTR style = GetWindowLongPtrA(content, GWL_STYLE);
    SetWindowLongPtrA(content, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);

    SetPropA(window, "AureaContentHWND", (HANDLE)content);

    DrawMenuBar(window);

    RECT client_rect;
    GetClientRect(window, &client_rect);
    
    LOG_DEBUG("ng_windows_set_window_content: Window client area = %dx%d", 
              client_rect.right - client_rect.left, 
              client_rect.bottom - client_rect.top);

    HMENU menu = GetMenu(window);
    int menu_height = 0;
    if (menu) {
        RECT menu_rect;
        if (GetMenuItemRect(window, menu, 0, &menu_rect)) {
            POINT pt_top = {menu_rect.left, menu_rect.top};
            POINT pt_bottom = {menu_rect.left, menu_rect.bottom};
            ScreenToClient(window, &pt_top);
            ScreenToClient(window, &pt_bottom);
            menu_height = pt_bottom.y - pt_top.y;
        } else {
            menu_height = GetSystemMetrics(SM_CYMENU);
        }
    }
    
    LOG_DEBUG("ng_windows_set_window_content: Menu height = %d", menu_height);

    int content_width = client_rect.right - client_rect.left;
    int content_height = client_rect.bottom - client_rect.top;

    LOG_INFO("ng_windows_set_window_content: Resizing content to %dx%d",
             content_width, content_height);

    SetWindowPos(content, NULL, 0, 0,
                 content_width, content_height,
                 SWP_NOZORDER | SWP_SHOWWINDOW);

    DrawMenuBar(window);

    ng_windows_refresh_dpi(window);

    extern void layout_box_children(HWND box);
    #define BOX_ORIENTATION_PROP "AureaBoxOrientation"
    if (GetPropA(content, BOX_ORIENTATION_PROP)) {
        LOG_INFO("ng_windows_set_window_content: Content is a box, ensuring it fills window width");
        SetWindowPos(content, NULL, 0, 0, content_width, content_height,
                    SWP_NOMOVE | SWP_NOZORDER);
        layout_box_children(content);
    }

    SetFocus(window);

    return NG_SUCCESS;
}

void ng_windows_window_set_title(NGHandle window, const char* title) {
    if (!window || !title) return;
    wchar_t* wide = ng_windows_utf8_to_wide(title);
    if (!wide) return;
    SetWindowTextW((HWND)window, wide);
    free(wide);
}

static HICON image_handle(
    const unsigned char* rgba,
    unsigned int width,
    unsigned int height, BOOL is_icon, unsigned int hot_x, unsigned int hot_y
) {
    if (!rgba || width == 0 || height == 0 || width > INT_MAX || height > INT_MAX) {
        return NULL;
    }

    BITMAPV5HEADER header = {0};
    header.bV5Size = sizeof(header);
    header.bV5Width = (LONG)width;
    header.bV5Height = -(LONG)height;
    header.bV5Planes = 1;
    header.bV5BitCount = 32;
    header.bV5Compression = BI_BITFIELDS;
    header.bV5RedMask = 0x00ff0000;
    header.bV5GreenMask = 0x0000ff00;
    header.bV5BlueMask = 0x000000ff;
    header.bV5AlphaMask = 0xff000000;

    void* dib_pixels = NULL;
    HDC dc = GetDC(NULL);
    HBITMAP color = CreateDIBSection(
        dc,
        (BITMAPINFO*)&header,
        DIB_RGB_COLORS,
        &dib_pixels,
        NULL,
        0
    );
    ReleaseDC(NULL, dc);
    if (!color || !dib_pixels) {
        if (color) DeleteObject(color);
        return NULL;
    }

    unsigned char* bgra = (unsigned char*)dib_pixels;
    size_t pixel_count = (size_t)width * (size_t)height;
    for (size_t i = 0; i < pixel_count; ++i) {
        bgra[i * 4] = (unsigned char)((rgba[i * 4 + 2] * rgba[i * 4 + 3] + 127) / 255);
        bgra[i * 4 + 1] = (unsigned char)((rgba[i * 4 + 1] * rgba[i * 4 + 3] + 127) / 255);
        bgra[i * 4 + 2] = (unsigned char)((rgba[i * 4] * rgba[i * 4 + 3] + 127) / 255);
        bgra[i * 4 + 3] = rgba[i * 4 + 3];
    }

    size_t mask_size = ((size_t)width + 15) / 16 * 2 * height;
    void* zeros = calloc(1, mask_size);
    HBITMAP mask = zeros ? CreateBitmap((int)width, (int)height, 1, 1, zeros) : NULL;
    free(zeros);
    if (!mask) {
        DeleteObject(color);
        return NULL;
    }

    ICONINFO info = {0};
    info.fIcon = is_icon;
    info.xHotspot = hot_x;
    info.yHotspot = hot_y;
    info.hbmColor = color;
    info.hbmMask = mask;
    HICON icon = CreateIconIndirect(&info);
    DeleteObject(mask);
    DeleteObject(color);
    return icon;
}

HICON ng_windows_icon_from_rgba(const unsigned char* rgba, unsigned int width, unsigned int height) {
    return image_handle(rgba, width, height, TRUE, 0, 0);
}

int ng_windows_window_set_icon_rgba(
    NGHandle window,
    const unsigned char* rgba,
    unsigned int width,
    unsigned int height
) {
    if (!window || !rgba || width == 0 || height == 0 ||
        width > INT_MAX || height > INT_MAX) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    HICON icon = ng_windows_icon_from_rgba(rgba, width, height);
    if (!icon) return NG_ERROR_PLATFORM_SPECIFIC;

    HWND hwnd = (HWND)window;
    HICON previous = (HICON)RemovePropA(hwnd, AUREA_WINDOW_ICON_PROPERTY);
    if (!SetPropA(hwnd, AUREA_WINDOW_ICON_PROPERTY, (HANDLE)icon)) {
        DestroyIcon(icon);
        if (previous) {
            SetPropA(hwnd, AUREA_WINDOW_ICON_PROPERTY, (HANDLE)previous);
        }
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    SendMessageA(hwnd, WM_SETICON, ICON_BIG, (LPARAM)icon);
    SendMessageA(hwnd, WM_SETICON, ICON_SMALL, (LPARAM)icon);
    if (previous) DestroyIcon(previous);
    return NG_SUCCESS;
}

int ng_windows_window_set_hide_on_close(NGHandle window, int hide) {
    if (!window) return NG_ERROR_INVALID_PARAMETER;
    HWND hwnd = (HWND)window;
    if (hide) {
        if (!SetPropA(hwnd, AUREA_HIDE_ON_CLOSE_PROPERTY, (HANDLE)1)) {
            return NG_ERROR_PLATFORM_SPECIFIC;
        }
    } else {
        RemovePropA(hwnd, AUREA_HIDE_ON_CLOSE_PROPERTY);
    }
    return NG_SUCCESS;
}

void ng_windows_window_set_close_guard(NGHandle window, int guarded) {
    if (!window) return;
    HWND hwnd = (HWND)window;
    if (guarded) {
        SetPropA(hwnd, AUREA_CLOSE_GUARD_PROPERTY, (HANDLE)1);
    } else {
        RemovePropA(hwnd, AUREA_CLOSE_GUARD_PROPERTY);
    }
}

void ng_windows_window_set_size(NGHandle window, int width, int height) {
    if (!window) return;
    ng_windows_set_client_size((HWND)window, width, height, 0);
}

int ng_windows_window_set_min_size(NGHandle window, int width, int height) {
    HWND hwnd = (HWND)window;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (width < 0 || height < 0) return NG_ERROR_INVALID_PARAMETER;
    SetPropA(hwnd, AUREA_MIN_CLIENT_W_PROP, (HANDLE)(INT_PTR)(width ? width + 1 : 0));
    SetPropA(hwnd, AUREA_MIN_CLIENT_H_PROP, (HANDLE)(INT_PTR)(height ? height + 1 : 0));
    int current_w = 0, current_h = 0;
    ng_windows_window_get_size(window, &current_w, &current_h);
    if (current_w < width || current_h < height) {
        ng_windows_set_client_size(hwnd, current_w > width ? current_w : width,
                                   current_h > height ? current_h : height, 0);
    }
    return NG_SUCCESS;
}

void ng_windows_window_get_size(NGHandle window, int* width, int* height) {
    if (!window || !width || !height) return;
    HWND hwnd = (HWND)window;
    RECT rect;
    GetClientRect(hwnd, &rect);
    *width = ng_windows_unscale(hwnd, rect.right - rect.left);
    *height = ng_windows_unscale(hwnd, rect.bottom - rect.top);
}

void ng_windows_window_set_position(NGHandle window, int x, int y) {
    if (!window) return;
    HWND hwnd = (HWND)window;
    SetWindowPos(hwnd, NULL, x, y, 0, 0, SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE);
}

void ng_windows_window_get_position(NGHandle window, int* x, int* y) {
    if (!window || !x || !y) return;
    HWND hwnd = (HWND)window;
    RECT rect;
    if (GetWindowRect(hwnd, &rect)) {
        *x = rect.left;
        *y = rect.top;
    }
}

void ng_windows_window_request_close(NGHandle window) {
    if (!window) return;
    HWND hwnd = (HWND)window;
    PostMessage(hwnd, WM_CLOSE, 0, 0);
}

int ng_windows_window_is_focused(NGHandle window) {
    if (!window) return 0;
    HWND hwnd = (HWND)window;
    return (GetForegroundWindow() == hwnd) ? 1 : 0;
}

static HWND g_cursor_hidden_by = NULL;

int ng_windows_window_set_cursor_visible(NGHandle window, int visible) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    HWND hwnd = (HWND)window;
    if (visible) {
        if (g_cursor_hidden_by) {
            ShowCursor(TRUE);
            g_cursor_hidden_by = NULL;
        }
    } else if (!g_cursor_hidden_by) {
        ShowCursor(FALSE);
        g_cursor_hidden_by = hwnd;
    } else {
        g_cursor_hidden_by = hwnd;
    }
    return NG_SUCCESS;
}

#define AUREA_CURSOR_ICON_PROP "AureaCursorIcon"
#define AUREA_CUSTOM_CURSOR_PROP "AureaCustomCursor"

void* ng_windows_window_cursor(NGHandle window) {
    HANDLE cursor = window ? GetPropA((HWND)window, AUREA_CURSOR_ICON_PROP) : NULL;
    return cursor ? (void*)cursor : (void*)LoadCursor(NULL, IDC_ARROW);
}

static int install_cursor(HWND hwnd, HCURSOR cursor, BOOL owned) {
    if (!cursor) return NG_ERROR_PLATFORM_SPECIFIC;
    HCURSOR previous = (HCURSOR)GetPropA(hwnd, AUREA_CUSTOM_CURSOR_PROP);
    if (!SetPropA(hwnd, AUREA_CURSOR_ICON_PROP, (HANDLE)cursor)) {
        if (owned) DestroyCursor(cursor);
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    if (owned && !SetPropA(hwnd, AUREA_CUSTOM_CURSOR_PROP, (HANDLE)cursor)) {
        SetPropA(hwnd, AUREA_CURSOR_ICON_PROP, (HANDLE)(previous ? previous : LoadCursor(NULL, IDC_ARROW)));
        DestroyCursor(cursor);
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    if (!owned) RemovePropA(hwnd, AUREA_CUSTOM_CURSOR_PROP);
    POINT pt;
    RECT client;
    if (GetCursorPos(&pt) && GetClientRect(hwnd, &client)) {
        HWND under = WindowFromPoint(pt);
        ScreenToClient(hwnd, &pt);
        if ((under == hwnd || IsChild(hwnd, under)) && PtInRect(&client, pt)) {
            SetCursor(cursor);
        }
    }
    if (previous) {
        if (GetCursor() == previous) SetCursor(cursor);
        DestroyCursor(previous);
    }
    return NG_SUCCESS;
}

int ng_windows_window_set_cursor_rgba(NGHandle window, const unsigned char* rgba, unsigned int width, unsigned int height, unsigned int hot_x, unsigned int hot_y) {
    if (!window || !rgba || hot_x >= width || hot_y >= height) return NG_ERROR_INVALID_PARAMETER;
    HCURSOR cursor = (HCURSOR)image_handle(rgba, width, height, FALSE, hot_x, hot_y);
    return install_cursor((HWND)window, cursor, TRUE);
}

int ng_windows_window_set_cursor_icon(NGHandle window, int icon) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    HWND hwnd = (HWND)window;
    LPCTSTR id;
    switch (icon) {
        case 1: id = IDC_HAND; break;
        case 2: id = IDC_IBEAM; break;
        case 3: id = IDC_CROSS; break;
        case 4: id = IDC_SIZEALL; break;
        case 5: id = IDC_SIZEWE; break;
        case 6: id = IDC_SIZENS; break;
        case 7: id = IDC_NO; break;
        case 8: id = IDC_WAIT; break;
        case 9: id = IDC_SIZENWSE; break;
        case 10: id = IDC_SIZENESW; break;
        default: id = IDC_ARROW; break;
    }
    HCURSOR cursor = LoadCursor(NULL, id);
    return install_cursor(hwnd, cursor, FALSE);
}

static HWND g_grab_holder = NULL;
static HWND g_raw_holder = NULL;

static void ng_windows_set_raw_mouse(HWND target) {
    RAWINPUTDEVICE rid;
    rid.usUsagePage = 0x01;
    rid.usUsage = 0x02;
    rid.dwFlags = target ? 0 : RIDEV_REMOVE;
    rid.hwndTarget = target;
    if (RegisterRawInputDevices(&rid, 1, sizeof(rid))) g_raw_holder = target;
}

static void ng_windows_clip_to_content(HWND hwnd) {
    RECT rect;
    GetClientRect(hwnd, &rect);
    POINT tl = { rect.left, rect.top };
    POINT br = { rect.right, rect.bottom };
    ClientToScreen(hwnd, &tl);
    ClientToScreen(hwnd, &br);
    rect.left = tl.x;
    rect.top = tl.y;
    rect.right = br.x;
    rect.bottom = br.y;
    ClipCursor(&rect);
}

static void ng_windows_give_up_grab(HWND hwnd) {
    if (g_grab_holder == hwnd) {
        ClipCursor(NULL);
        g_grab_holder = NULL;
    }
    if (g_raw_holder == hwnd) ng_windows_set_raw_mouse(NULL);
}

static void ng_windows_take_grab(HWND hwnd) {
    INT_PTR mode = (INT_PTR)GetPropA(hwnd, AUREA_CURSOR_GRAB_PROP);
    if (mode == 0 || IsIconic(hwnd)) return;
    if (g_grab_holder && g_grab_holder != hwnd) ng_windows_give_up_grab(g_grab_holder);
    ng_windows_clip_to_content(hwnd);
    g_grab_holder = hwnd;
    if (mode == 2) {
        if (g_raw_holder != hwnd) ng_windows_set_raw_mouse(hwnd);
    } else if (g_raw_holder == hwnd) {
        ng_windows_set_raw_mouse(NULL);
    }
}

void ng_windows_cursor_grab_activate(void* window, int active) {
    HWND hwnd = (HWND)window;
    if (active) {
        ng_windows_take_grab(hwnd);
    } else {
        ng_windows_give_up_grab(hwnd);
    }
}

void ng_windows_cursor_grab_refresh(void* window) {
    HWND hwnd = (HWND)window;
    if (g_grab_holder != hwnd) return;
    if (IsIconic(hwnd)) {
        ng_windows_give_up_grab(hwnd);
    } else {
        ng_windows_clip_to_content(hwnd);
    }
}

void ng_windows_release_window_input(void* window) {
    HWND hwnd = (HWND)window;
    ng_windows_give_up_grab(hwnd);
    HCURSOR cursor = (HCURSOR)RemovePropA(hwnd, AUREA_CUSTOM_CURSOR_PROP);
    if (cursor) {
        if (GetCursor() == cursor) SetCursor(LoadCursor(NULL, IDC_ARROW));
        DestroyCursor(cursor);
    }
    RemovePropA(hwnd, AUREA_CURSOR_ICON_PROP);
    RemovePropA(hwnd, AUREA_CURSOR_GRAB_PROP);
    if (g_cursor_hidden_by == hwnd) {
        ShowCursor(TRUE);
        g_cursor_hidden_by = NULL;
    }
}

int ng_windows_window_set_cursor_grab(NGHandle window, int mode) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    HWND hwnd = (HWND)window;

    if (mode == 0) {
        RemovePropA(hwnd, AUREA_CURSOR_GRAB_PROP);
        ng_windows_give_up_grab(hwnd);
        return NG_SUCCESS;
    }

    SetPropA(hwnd, AUREA_CURSOR_GRAB_PROP, (HANDLE)(INT_PTR)mode);
    if (GetActiveWindow() == hwnd) ng_windows_take_grab(hwnd);
    return NG_SUCCESS;
}

char* ng_windows_get_clipboard_text(void) {
    if (!OpenClipboard(NULL)) return NULL;
    HANDLE h = GetClipboardData(CF_UNICODETEXT);
    if (!h) { CloseClipboard(); return NULL; }
    WCHAR* wide = (WCHAR*)GlobalLock(h);
    if (!wide) { CloseClipboard(); return NULL; }
    SIZE_T capacity = GlobalSize(h) / sizeof(WCHAR);
    SIZE_T units = 0;
    while (units < capacity && wide[units]) ++units;
    if (units == capacity || units > INT_MAX) {
        GlobalUnlock(h); CloseClipboard(); return NULL;
    }
    int len = units ? WideCharToMultiByte(CP_UTF8, 0, wide, (int)units, NULL, 0, NULL, NULL) : 0;
    char* result = NULL;
    if (!units || len > 0) {
        result = (char*)malloc((size_t)len + 1);
        if (result) {
            if (len) WideCharToMultiByte(CP_UTF8, 0, wide, (int)units, result, len, NULL, NULL);
            int out = 0;
            for (int i = 0; i < len; i++) {
                if (result[i] == '\r' && i + 1 < len && result[i + 1] == '\n') continue;
                result[out++] = result[i];
            }
            result[out] = 0;
        }
    }
    GlobalUnlock(h);
    CloseClipboard();
    return result;
}

void ng_windows_free_clipboard_text(char* text) {
    free(text);
}

int ng_windows_set_clipboard_text(const char* text) {
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    int wlen = MultiByteToWideChar(CP_UTF8, 0, text, -1, NULL, 0);
    if (wlen <= 0) return NG_ERROR_PLATFORM_SPECIFIC;
    int breaks = 0;
    for (const char* c = text; *c; c++) breaks += *c == '\n' && (c == text || c[-1] != '\r');
    HGLOBAL h = GlobalAlloc(GMEM_MOVEABLE, ((SIZE_T)wlen + (SIZE_T)breaks) * sizeof(WCHAR));
    if (!h) return NG_ERROR_PLATFORM_SPECIFIC;
    WCHAR* dst = (WCHAR*)GlobalLock(h);
    if (!dst) { GlobalFree(h); return NG_ERROR_PLATFORM_SPECIFIC; }
    MultiByteToWideChar(CP_UTF8, 0, text, -1, dst + breaks, wlen);
    WCHAR* at = dst;
    for (WCHAR* from = dst + breaks; ; from++) {
        if (*from == L'\n' && (at == dst || at[-1] != L'\r')) *at++ = L'\r';
        *at++ = *from;
        if (!*from) break;
    }
    GlobalUnlock(h);
    HWND owner = (HWND)ng_windows_clipboard_owner();
    if (!owner) { GlobalFree(h); return NG_ERROR_PLATFORM_SPECIFIC; }
    if (!OpenClipboard(owner)) {
        int error = GetLastError() == ERROR_ACCESS_DENIED ? NG_ERROR_BUSY : NG_ERROR_PLATFORM_SPECIFIC;
        ng_windows_clipboard_cleanup(owner); GlobalFree(h); return error;
    }
    if (!EmptyClipboard()) { CloseClipboard(); ng_windows_clipboard_cleanup(owner); GlobalFree(h); return NG_ERROR_PLATFORM_SPECIFIC; }
    if (!SetClipboardData(CF_UNICODETEXT, h)) {
        CloseClipboard();
        ng_windows_clipboard_cleanup(owner);
        GlobalFree(h);
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    CloseClipboard();
    ng_windows_clipboard_cleanup(owner);
    return NG_SUCCESS;
}
