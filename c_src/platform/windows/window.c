#include "window.h"
#include "utils.h"
#include "../common/errors.h"
#include <windows.h>

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

void ng_windows_destroy_window(NGHandle handle) {
    if (!handle) return;
    DestroyWindow((HWND)handle);
}

int ng_windows_set_window_content(NGHandle window_handle, NGHandle content_handle) {
    if (!window_handle || !content_handle) return NG_ERROR_INVALID_HANDLE;
    
    HWND window = (HWND)window_handle;
    HWND content = (HWND)content_handle;
    
    // Set parent and show the content window
    SetParent(content, window);
    
    // Get client area and resize content to fill it
    RECT client_rect;
    GetClientRect(window, &client_rect);
    SetWindowPos(content, NULL, 0, 0, 
                 client_rect.right - client_rect.left,
                 client_rect.bottom - client_rect.top,
                 SWP_NOZORDER | SWP_SHOWWINDOW);
    
    return NG_SUCCESS;
}

