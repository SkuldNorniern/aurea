#include "common.h"
#include "../elements.h"
#include <windows.h>

NGHandle ng_windows_create_text_view(int is_editable) {
    return (NGHandle)ng_windows_create_rich_text(is_editable);
}

void ng_windows_text_view_invalidate(NGHandle text_view) {
    if (!text_view) return;
    InvalidateRect((HWND)text_view, NULL, FALSE);
}
