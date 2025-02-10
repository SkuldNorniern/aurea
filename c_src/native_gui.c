#include "native_gui.h"
#include <stdlib.h>

#ifdef _WIN32
#include <windows.h>

static WNDCLASSEXA g_wc = {0};
static const char* CLASS_NAME = "FenestraWindow";

NGHandle ng_create_window(const char* title, int width, int height) {
    // Register window class if not already registered
    if (!g_wc.lpszClassName) {
        g_wc.cbSize = sizeof(WNDCLASSEXA);
        g_wc.lpfnWndProc = DefWindowProcA;
        g_wc.hInstance = GetModuleHandleA(NULL);
        g_wc.lpszClassName = CLASS_NAME;
        
        if (!RegisterClassExA(&g_wc)) {
            return NULL;
        }
    }
    
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

void ng_destroy_window(NGHandle handle) {
    if (handle) {
        DestroyWindow((HWND)handle);
    }
}

NGMenuHandle ng_create_menu_handle(void) {
    HMENU hmenu = CreateMenu();
    return (NGMenuHandle)hmenu;
}

void ng_destroy_menu_handle(NGMenuHandle handle) {
    if (handle) {
        DestroyMenu((HMENU)handle);
    }
}

int ng_attach_menu_to_window(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return -1;
    if (!SetMenu((HWND)window, (HMENU)menu)) return -1;
    return 0;
}

int ng_add_raw_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return -1;
    
    MENUITEMINFOA mii = {0};
    mii.cbSize = sizeof(MENUITEMINFOA);
    mii.fMask = MIIM_STRING | MIIM_ID;
    mii.wID = id;
    mii.dwTypeData = (LPSTR)title;
    
    return InsertMenuItemA((HMENU)menu, -1, TRUE, &mii) ? 0 : -1;
}

int ng_handle_menu_event(NGMenuHandle menu, unsigned int id) {
    // Windows handles menu events through WM_COMMAND
    return 0;
}

#elif defined(__APPLE__)
#import <Cocoa/Cocoa.h>

// Basic macOS stubs - these need to be implemented with proper Cocoa code
NGHandle ng_create_window(const char* title, int width, int height) {
    return NULL;
}

void ng_destroy_window(NGHandle handle) {
}

NGMenuHandle ng_create_menu_handle(void) {
    return NULL;
}

void ng_destroy_menu_handle(NGMenuHandle handle) {
}

int ng_attach_menu_to_window(NGHandle window, NGMenuHandle menu) {
    return -1;
}

int ng_add_raw_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    return -1;
}

int ng_handle_menu_event(NGMenuHandle menu, unsigned int id) {
    return -1;
}

#else // Linux (GTK)
#include <gtk/gtk.h>

// Basic Linux/GTK stubs - these need to be implemented with proper GTK code
NGHandle ng_create_window(const char* title, int width, int height) {
    return NULL;
}

void ng_destroy_window(NGHandle handle) {
}

NGMenuHandle ng_create_menu_handle(void) {
    return NULL;
}

void ng_destroy_menu_handle(NGMenuHandle handle) {
}

int ng_attach_menu_to_window(NGHandle window, NGMenuHandle menu) {
    return -1;
}

int ng_add_raw_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    return -1;
}

int ng_handle_menu_event(NGMenuHandle menu, unsigned int id) {
    return -1;
}

#endif 