#include "elements.h"
#include "../common/errors.h"
#include <windows.h>
#include <commctrl.h>
#include <richedit.h>

NGHandle ng_windows_create_button(const char* title) {
    if (!title) return NULL;
    
    HWND button = CreateWindowExA(
        0,
        "BUTTON",
        title,
        WS_VISIBLE | WS_CHILD | BS_PUSHBUTTON,
        0, 0, 100, 30,
        NULL,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    return (NGHandle)button;
}

NGHandle ng_windows_create_label(const char* text) {
    if (!text) return NULL;
    
    HWND label = CreateWindowExA(
        0,
        "STATIC",
        text,
        WS_VISIBLE | WS_CHILD | SS_LEFT,
        0, 0, 100, 20,
        NULL,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    return (NGHandle)label;
}

NGHandle ng_windows_create_box(int is_vertical) {
    // Windows doesn't have a native box widget, use a container window
    HWND container = CreateWindowExA(
        0,
        "STATIC",
        NULL,
        WS_VISIBLE | WS_CHILD,
        0, 0, 100, 100,
        NULL,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    return (NGHandle)container;
}

int ng_windows_box_add(NGHandle box, NGHandle element) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;
    
    // Set parent of element to box
    SetParent((HWND)element, (HWND)box);
    ShowWindow((HWND)element, SW_SHOW);
    
    return NG_SUCCESS;
}

NGHandle ng_windows_create_text_editor(void) {
    // Load RichEdit library
    LoadLibraryA("riched20.dll");
    
    HWND edit = CreateWindowExA(
        WS_EX_CLIENTEDGE,
        "RichEdit20A",
        "",
        WS_VISIBLE | WS_CHILD | WS_VSCROLL | WS_HSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL,
        0, 0, 100, 100,
        NULL,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    return (NGHandle)edit;
}

NGHandle ng_windows_create_text_view(int is_editable) {
    // Load RichEdit library
    LoadLibraryA("riched20.dll");
    
    DWORD style = WS_VISIBLE | WS_CHILD | WS_VSCROLL | WS_HSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL;
    if (!is_editable) {
        style |= ES_READONLY;
    }
    
    HWND edit = CreateWindowExA(
        WS_EX_CLIENTEDGE,
        "RichEdit20A",
        "",
        style,
        0, 0, 100, 100,
        NULL,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    return (NGHandle)edit;
}

int ng_windows_set_text_content(NGHandle text_handle, const char* content) {
    if (!text_handle || !content) return NG_ERROR_INVALID_PARAMETER;
    
    SetWindowTextA((HWND)text_handle, content);
    return NG_SUCCESS;
}

char* ng_windows_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;
    
    int len = GetWindowTextLengthA((HWND)text_handle);
    if (len == 0) {
        char* empty = (char*)malloc(1);
        if (empty) empty[0] = '\0';
        return empty;
    }
    
    char* buffer = (char*)malloc(len + 1);
    if (!buffer) return NULL;
    
    GetWindowTextA((HWND)text_handle, buffer, len + 1);
    return buffer;
}

void ng_windows_free_text_content(char* content) {
    if (content) {
        free(content);
    }
}

NGHandle ng_windows_create_canvas(int width, int height) {
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

void ng_windows_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    InvalidateRect((HWND)canvas, NULL, FALSE);
}

