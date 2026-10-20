#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>
#include <richedit.h>

NGHandle ng_windows_create_text_editor(unsigned int id) {
    LoadLibraryW(L"riched20.dll");

    HWND edit = ng_windows_create_control(
        WS_EX_CLIENTEDGE, RICHEDIT_CLASSW, "",
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL,
        400, 100, ng_windows_detached_parent(), 0);

    if (edit && id != 0) {
        ng_windows_watch_text(edit, id, TEXT_CHANGE_EDITOR);
    }

    return (NGHandle)edit;
}

void ng_windows_text_editor_invalidate(NGHandle text_editor) {
    if (!text_editor) return;
    InvalidateRect((HWND)text_editor, NULL, FALSE);
}
