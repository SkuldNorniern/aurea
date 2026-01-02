#include "common.h"
#include "../elements.h"
#include "../../../common/errors.h"
#include <windows.h>
#include <stdlib.h>

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

NGHandle ng_windows_create_text_field(void) {
    HWND temp_parent = GetDesktopWindow();

    HWND edit = CreateWindowExA(
        WS_EX_CLIENTEDGE,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL,
        0, 0, 200, 24,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    return (NGHandle)edit;
}

