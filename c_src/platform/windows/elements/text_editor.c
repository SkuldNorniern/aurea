#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>
#include <commctrl.h>
#include <richedit.h>

extern void ng_invoke_text_callback(unsigned int id, const char* content);

static WNDPROC g_old_text_editor_proc = NULL;

static LRESULT CALLBACK text_editor_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    if (msg == WM_COMMAND && HIWORD(wParam) == EN_CHANGE) {
        unsigned int id = (unsigned int)GetWindowLongPtrA(hwnd, GWLP_USERDATA);
        if (id != 0) {
            int len = GetWindowTextLengthA(hwnd);
            if (len > 0) {
                char* buffer = (char*)malloc(len + 1);
                if (buffer) {
                    GetWindowTextA(hwnd, buffer, len + 1);
                    ng_invoke_text_callback(id, buffer);
                    free(buffer);
                }
            } else {
                ng_invoke_text_callback(id, "");
            }
        }
    }
    if (g_old_text_editor_proc) {
        return CallWindowProcA(g_old_text_editor_proc, hwnd, msg, wParam, lParam);
    }
    return DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_text_editor(unsigned int id) {
    LoadLibraryA("riched20.dll");

    HWND temp_parent = GetDesktopWindow();

    HWND edit = CreateWindowExA(
        WS_EX_CLIENTEDGE,
        "RichEdit20A",
        "",
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL,
        0, 0, 400, 100,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (edit && id != 0) {
        SetWindowLongPtrA(edit, GWLP_USERDATA, (LONG_PTR)id);
        g_old_text_editor_proc = (WNDPROC)SetWindowLongPtrA(edit, GWLP_WNDPROC, (LONG_PTR)text_editor_proc);
    }

    return (NGHandle)edit;
}

void ng_windows_text_editor_invalidate(NGHandle text_editor) {
    if (!text_editor) return;
    InvalidateRect((HWND)text_editor, NULL, FALSE);
}

