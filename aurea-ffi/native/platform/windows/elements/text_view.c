#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>
#include <richedit.h>

NGHandle ng_windows_create_text_view(int is_editable, unsigned int id) {
    LoadLibraryW(L"riched20.dll");

    DWORD style = WS_CHILD | WS_VSCROLL | WS_HSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL;
    if (!is_editable) {
        style |= ES_READONLY;
    }

    HWND edit = ng_windows_create_control(
        WS_EX_CLIENTEDGE, RICHEDIT_CLASSW, "", style | WS_VISIBLE,
        400, 100, ng_windows_detached_parent(), 0);

    if (edit) ng_windows_unlimit_text(edit);
    if (edit && id != 0 && is_editable) {
        ng_windows_watch_text(edit, id, TEXT_CHANGE_VIEW);
    }

    return (NGHandle)edit;
}

void ng_windows_text_view_invalidate(NGHandle text_view) {
    if (!text_view) return;
    InvalidateRect((HWND)text_view, NULL, FALSE);
}
