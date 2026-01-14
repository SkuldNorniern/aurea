#include "window.h"
#include "utils.h"
#include "common/errors.h"
#include <windows.h>
#include <ShellScalingApi.h>
#include <stdio.h>

// Logging function declarations (implemented in Rust)
extern void ng_log_error(const char* msg);
extern void ng_log_warn(const char* msg);
extern void ng_log_info(const char* msg);
extern void ng_log_debug(const char* msg);
extern void ng_log_trace(const char* msg);

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
    }
    
    return (NGHandle)hwnd;
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
    DestroyWindow((HWND)handle);
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

    return NG_SUCCESS;
}

void ng_windows_window_set_title(NGHandle window, const char* title) {
    if (!window || !title) return;
    HWND hwnd = (HWND)window;
    SetWindowTextA(hwnd, title);
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

