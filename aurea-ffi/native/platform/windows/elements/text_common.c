#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include <windows.h>
#include <stdlib.h>

int ng_windows_set_text_content(NGHandle text_handle, const char* content) {
    if (!text_handle || !content) return NG_ERROR_INVALID_PARAMETER;
    return ng_windows_set_window_text((HWND)text_handle, content);
}

char* ng_windows_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;
    return ng_windows_window_text((HWND)text_handle);
}

void ng_windows_free_text_content(char* content) {
    if (content) {
        free(content);
    }
}

NGHandle ng_windows_create_text_field(void) {
    return (NGHandle)ng_windows_create_control(
        WS_EX_CLIENTEDGE, L"EDIT", "",
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL,
        200, 24, ng_windows_detached_parent(), 0);
}
