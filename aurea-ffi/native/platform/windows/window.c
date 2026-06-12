#include "window.h"
#include "utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <ShellScalingApi.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>

static const char* AUREA_WINDOW_ICON_PROPERTY = "AureaWindowIcon";

// Helper macro for formatted logging
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

NGHandle ng_windows_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    
    const char* class_name = ng_windows_get_class_name();
    
    HWND hwnd = CreateWindowExA(
        0,
        class_name,
        title,
        WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT, CW_USEDEFAULT,
        width, height,
        NULL,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    if (hwnd) {
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        /* Explicitly give keyboard focus to the window so key events arrive
           immediately without requiring a user click first. */
        SetFocus(hwnd);
    }

    return (NGHandle)hwnd;
}

NGHandle ng_windows_create_window_with_type(const char* title, int width, int height, int window_type) {
    (void)window_type;
    return ng_windows_create_window(title, width, height);
}

float ng_windows_get_scale_factor(NGHandle window) {
    if (!window) return 1.0f;
    HWND hwnd = (HWND)window;
    
    // Try GetDpiForWindow first (Windows 10 1607+)
    typedef UINT (WINAPI *GetDpiForWindowFunc)(HWND);
    HMODULE user32 = GetModuleHandleA("user32.dll");
    if (user32) {
        GetDpiForWindowFunc getDpiForWindow = (GetDpiForWindowFunc)GetProcAddress(user32, "GetDpiForWindow");
        if (getDpiForWindow) {
            UINT dpi = getDpiForWindow(hwnd);
            if (dpi > 0) {
                return (float)dpi / 96.0f;
            }
        }
    }
    
    // Fallback to GetDpiForMonitor
    HMONITOR monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    if (monitor) {
        UINT dpiX = 96;
        UINT dpiY = 96;
        if (GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &dpiX, &dpiY) == S_OK) {
            return (float)dpiX / 96.0f;
        }
    }
    return 1.0f;
}

void ng_windows_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    if (!window) return;
    HWND hwnd = (HWND)window;
    
    // Enable DPI awareness for the window
    typedef BOOL (WINAPI *EnableNonClientDpiScalingFunc)(HWND);
    HMODULE user32 = GetModuleHandleA("user32.dll");
    if (user32) {
        EnableNonClientDpiScalingFunc enableNonClientDpiScaling = 
            (EnableNonClientDpiScalingFunc)GetProcAddress(user32, "EnableNonClientDpiScaling");
        if (enableNonClientDpiScaling) {
            enableNonClientDpiScaling(hwnd);
        }
    }
    
    // Register callback
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
    DestroyWindow(hwnd);
}

void ng_windows_window_show(NGHandle window) {
    if (!window) return;
    ShowWindow((HWND)window, SW_SHOW);
    UpdateWindow((HWND)window);
}

void ng_windows_window_hide(NGHandle window) {
    if (!window) return;
    ShowWindow((HWND)window, SW_HIDE);
}

int ng_windows_window_is_visible(NGHandle window) {
    if (!window) return 0;
    return IsWindowVisible((HWND)window) ? 1 : 0;
}

NGHandle ng_windows_window_get_content_view(NGHandle window) {
    // On Win32, child controls are parented directly to the window HWND.
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

    // Set parent (this automatically makes it a child window)
    SetParent(content, window);

    // Add WS_CHILD style if not already present
    LONG_PTR style = GetWindowLongPtrA(content, GWL_STYLE);
    SetWindowLongPtrA(content, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);

    // Get client area - on Windows, this already excludes the menu bar area
    RECT client_rect;
    GetClientRect(window, &client_rect);
    
    LOG_DEBUG("ng_windows_set_window_content: Window client area = %dx%d", 
              client_rect.right - client_rect.left, 
              client_rect.bottom - client_rect.top);

    // Account for menu bar height - calculate the actual menu bar area
    HMENU menu = GetMenu(window);
    int menu_height = 0;
    if (menu) {
        // Try to get the menu bar rectangle for accurate positioning
        RECT menu_rect;
        if (GetMenuItemRect(window, menu, 0, &menu_rect)) {
            // Convert menu rect coordinates to client coordinates
            POINT pt_top = {menu_rect.left, menu_rect.top};
            POINT pt_bottom = {menu_rect.left, menu_rect.bottom};
            ScreenToClient(window, &pt_top);
            ScreenToClient(window, &pt_bottom);
            // Menu height is the difference between bottom and top in client coordinates
            menu_height = pt_bottom.y - pt_top.y;
        } else {
            // Fallback to system metrics
            menu_height = GetSystemMetrics(SM_CYMENU);
        }
    }
    
    LOG_DEBUG("ng_windows_set_window_content: Menu height = %d", menu_height);

    int content_width = client_rect.right - client_rect.left;
    int content_height = client_rect.bottom - client_rect.top;

    LOG_INFO("ng_windows_set_window_content: Resizing content to %dx%d",
             content_width, content_height);

    // Position content to fill the entire client area (which already excludes menu bar)
    SetWindowPos(content, NULL, 0, 0,
                 content_width, content_height,
                 SWP_NOZORDER | SWP_SHOWWINDOW);

    // Force redraw of menu bar
    DrawMenuBar(window);

    // If content is a box, ensure it's properly sized and layout its children
    extern void layout_box_children(HWND box);
    #define BOX_ORIENTATION_PROP "AureaBoxOrientation"
    if (GetPropA(content, BOX_ORIENTATION_PROP)) {
        LOG_INFO("ng_windows_set_window_content: Content is a box, ensuring it fills window width");
        // Force box to fill window width
        SetWindowPos(content, NULL, 0, 0, content_width, content_height,
                    SWP_NOMOVE | SWP_NOZORDER);
        // Layout children to fill width
        layout_box_children(content);
    }

    /* Store the content handle so WM_SIZE can resize it automatically. */
    SetPropA(window, "AureaContentHWND", (HANDLE)content);

    /* Return keyboard focus to the NativeGuiWindow.  SetParent calls inside
       box_add / set_window_content can silently move focus to a child HWND. */
    SetFocus(window);

    return NG_SUCCESS;
}

void ng_windows_window_set_title(NGHandle window, const char* title) {
    if (!window || !title) return;
    HWND hwnd = (HWND)window;
    SetWindowTextA(hwnd, title);
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
        return NG_ERROR_PLATFORM_SPECIFIC;
    }

    unsigned char* bgra = (unsigned char*)dib_pixels;
    size_t pixel_count = (size_t)width * (size_t)height;
    for (size_t i = 0; i < pixel_count; ++i) {
        bgra[i * 4] = rgba[i * 4 + 2];
        bgra[i * 4 + 1] = rgba[i * 4 + 1];
        bgra[i * 4 + 2] = rgba[i * 4];
        bgra[i * 4 + 3] = rgba[i * 4 + 3];
    }

    HBITMAP mask = CreateBitmap((int)width, (int)height, 1, 1, NULL);
    if (!mask) {
        DeleteObject(color);
        return NG_ERROR_PLATFORM_SPECIFIC;
    }

    ICONINFO info = {0};
    info.fIcon = TRUE;
    info.hbmColor = color;
    info.hbmMask = mask;
    HICON icon = CreateIconIndirect(&info);
    DeleteObject(mask);
    DeleteObject(color);
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

void ng_windows_window_set_size(NGHandle window, int width, int height) {
    if (!window) return;
    HWND hwnd = (HWND)window;
    RECT rect;
    GetWindowRect(hwnd, &rect);
    int x = rect.left;
    int y = rect.top;
    SetWindowPos(hwnd, NULL, x, y, width, height, SWP_NOZORDER | SWP_NOACTIVATE);
}

void ng_windows_window_get_size(NGHandle window, int* width, int* height) {
    if (!window || !width || !height) return;
    HWND hwnd = (HWND)window;
    RECT rect;
    GetClientRect(hwnd, &rect);
    *width = rect.right - rect.left;
    *height = rect.bottom - rect.top;
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

int ng_windows_window_set_cursor_visible(NGHandle window, int visible) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    int count = 0;
    int limit = 32;

    if (visible) {
        while (count < limit && ShowCursor(TRUE) < 0) {
            count++;
        }
    } else {
        while (count < limit && ShowCursor(FALSE) >= 0) {
            count++;
        }
    }

    return NG_SUCCESS;
}

int ng_windows_window_set_cursor_grab(NGHandle window, int mode) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    HWND hwnd = (HWND)window;

    if (mode == 0) {
        ClipCursor(NULL);
        RemovePropA(hwnd, AUREA_CURSOR_GRAB_PROP);

        RAWINPUTDEVICE rid;
        rid.usUsagePage = 0x01;
        rid.usUsage = 0x02;
        rid.dwFlags = RIDEV_REMOVE;
        rid.hwndTarget = NULL;
        RegisterRawInputDevices(&rid, 1, sizeof(rid));

        return NG_SUCCESS;
    }

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
    SetPropA(hwnd, AUREA_CURSOR_GRAB_PROP, (HANDLE)(INT_PTR)mode);

    if (mode == 2) {
        RAWINPUTDEVICE rid;
        rid.usUsagePage = 0x01;
        rid.usUsage = 0x02;
        rid.dwFlags = RIDEV_INPUTSINK;
        rid.hwndTarget = hwnd;
        RegisterRawInputDevices(&rid, 1, sizeof(rid));
    }

    return NG_SUCCESS;
}

char* ng_windows_get_clipboard_text(void) {
    if (!OpenClipboard(NULL)) return NULL;
    HANDLE h = GetClipboardData(CF_UNICODETEXT);
    if (!h) { CloseClipboard(); return NULL; }
    WCHAR* wide = (WCHAR*)GlobalLock(h);
    if (!wide) { CloseClipboard(); return NULL; }
    int len = WideCharToMultiByte(CP_UTF8, 0, wide, -1, NULL, 0, NULL, NULL);
    char* result = NULL;
    if (len > 0) {
        result = (char*)malloc((size_t)len);
        if (result) WideCharToMultiByte(CP_UTF8, 0, wide, -1, result, len, NULL, NULL);
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
    HGLOBAL h = GlobalAlloc(GMEM_MOVEABLE, (SIZE_T)wlen * sizeof(WCHAR));
    if (!h) return NG_ERROR_PLATFORM_SPECIFIC;
    WCHAR* dst = (WCHAR*)GlobalLock(h);
    if (!dst) { GlobalFree(h); return NG_ERROR_PLATFORM_SPECIFIC; }
    MultiByteToWideChar(CP_UTF8, 0, text, -1, dst, wlen);
    GlobalUnlock(h);
    if (!OpenClipboard(NULL)) { GlobalFree(h); return NG_ERROR_PLATFORM_SPECIFIC; }
    EmptyClipboard();
    if (!SetClipboardData(CF_UNICODETEXT, h)) {
        CloseClipboard();
        GlobalFree(h);
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    CloseClipboard();
    return NG_SUCCESS;
}
