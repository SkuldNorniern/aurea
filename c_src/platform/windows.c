#include "windows.h"
#include "../common/errors.h"
#include <windows.h>

static WNDCLASSEXA g_wc = {0};
static const char* CLASS_NAME = "NativeGuiWindow";
static BOOL win_initialized = FALSE;

// Callback function for window messages
LRESULT CALLBACK WindowProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam) {
    switch (uMsg) {
        case WM_COMMAND:
            // Handle menu item clicks
            if (HIWORD(wParam) == 0) { // Menu item clicked
                int id = LOWORD(wParam);
                // Log menu click for debugging
                OutputDebugStringA("Menu item clicked: ");
                char buf[32];
                sprintf_s(buf, sizeof(buf), "%d\n", id);
                OutputDebugStringA(buf);
            }
            break;
        
        case WM_DESTROY:
            PostQuitMessage(0);
            return 0;
    }
    return DefWindowProcA(hwnd, uMsg, wParam, lParam);
}

int ng_platform_init(void) {
    if (!win_initialized) {
        g_wc.cbSize = sizeof(WNDCLASSEXA);
        g_wc.lpfnWndProc = WindowProc; // Use our window procedure
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

void ng_platform_cleanup(void) {
    if (win_initialized) {
        UnregisterClassA(CLASS_NAME, g_wc.hInstance);
        win_initialized = FALSE;
    }
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    
    HWND hwnd = CreateWindowExA(
        0,
        CLASS_NAME,
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

void ng_platform_destroy_window(NGHandle handle) {
    if (!handle) return;
    DestroyWindow((HWND)handle);
}

NGMenuHandle ng_platform_create_menu(void) {
    HMENU menubar = CreateMenu();
    return (NGMenuHandle)menubar;
}

void ng_platform_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    DestroyMenu((HMENU)handle);
}

int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;
    
    if (!SetMenu((HWND)window, (HMENU)menu)) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    DrawMenuBar((HWND)window);
    return NG_SUCCESS;
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle parent_menu, const char* title) {
    if (!parent_menu || !title) return NULL;
    
    HMENU submenu = CreatePopupMenu();
    if (!submenu) return NULL;
    
    if (!AppendMenuA((HMENU)parent_menu, MF_STRING | MF_POPUP, (UINT_PTR)submenu, title)) {
        DestroyMenu(submenu);
        return NULL;
    }
    
    return (NGMenuHandle)submenu;
}

int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    
    // Windows uses command IDs for menu items, starting from 1
    UINT command_id = id + 1;
    
    if (!AppendMenuA((HMENU)menu, MF_STRING, command_id, title)) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    return NG_SUCCESS;
}

int ng_platform_run(void) {
    MSG msg;
    while (GetMessageA(&msg, NULL, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessageA(&msg);
    }
    return NG_SUCCESS;
}

NGHandle ng_platform_create_canvas(int width, int height) {
    // Create a child window for custom rendering
    // This will be extended to support DirectX/OpenGL
    HWND hwnd = CreateWindowExA(
        0,
        "STATIC",
        NULL,
        WS_CHILD | WS_VISIBLE,
        0, 0, width, height,
        NULL,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    return (NGHandle)hwnd;
}

void ng_platform_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    InvalidateRect((HWND)canvas, NULL, FALSE);
}

// ... rest of Windows implementation ... 