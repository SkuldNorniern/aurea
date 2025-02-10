#include "windows.h"
#include "../common/errors.h"
#include <windows.h>

static WNDCLASSEXA g_wc = {0};
static const char* CLASS_NAME = "NativeGuiWindow";
static BOOL win_initialized = FALSE;

int ng_platform_init(void) {
    if (!win_initialized) {
        g_wc.cbSize = sizeof(WNDCLASSEXA);
        g_wc.lpfnWndProc = DefWindowProcA;
        g_wc.hInstance = GetModuleHandleA(NULL);
        g_wc.lpszClassName = CLASS_NAME;
        
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
    HMENU menu = CreateMenu();
    return (NGMenuHandle)menu;
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

int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    
    if (!AppendMenuA((HMENU)menu, MF_STRING, id, title)) {
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

// ... rest of Windows implementation ... 