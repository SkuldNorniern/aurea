#include "common.h"
#include "../elements.h"
#include <windows.h>

NGHandle ng_windows_create_text_editor(void) {
    return (NGHandle)ng_windows_create_rich_text(1);
}

void ng_windows_text_editor_invalidate(NGHandle text_editor) {
    if (!text_editor) return;
    InvalidateRect((HWND)text_editor, NULL, FALSE);
}
