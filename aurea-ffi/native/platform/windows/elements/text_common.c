#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include "common/platform_api.h"
#include <windows.h>
#include <richedit.h>
#include <stdlib.h>
#include <string.h>

int ng_windows_is_rich_edit(HWND hwnd) {
    wchar_t cls[32];
    return GetClassNameW(hwnd, cls, 32) &&
           (_wcsicmp(cls, MSFTEDIT_CLASS) == 0 || _wcsicmp(cls, RICHEDIT_CLASSW) == 0);
}

static int is_rich(HWND hwnd) {
    return ng_windows_is_rich_edit(hwnd);
}

static int is_text(HWND hwnd) {
    wchar_t cls[32];
    return hwnd && IsWindow(hwnd) &&
           (is_rich(hwnd) || (GetClassNameW(hwnd, cls, 32) && _wcsicmp(cls, L"EDIT") == 0));
}

static char* normalized(wchar_t* wide, int len) {
    int out = 0;
    for (int i = 0; i < len; i++) {
        if (wide[i] == L'\r') {
            if (i + 1 < len && wide[i + 1] == L'\n') i++;
            wide[out++] = L'\n';
        } else {
            wide[out++] = wide[i];
        }
    }
    return ng_windows_wide_to_utf8(wide, out);
}

char* ng_windows_text_content(HWND hwnd) {
    int len = GetWindowTextLengthW(hwnd);
    wchar_t* wide = (wchar_t*)malloc(((size_t)len + 1) * sizeof(wchar_t));
    if (!wide) return NULL;
    len = len > 0 ? GetWindowTextW(hwnd, wide, len + 1) : 0;
    char* utf8 = normalized(wide, len);
    free(wide);
    return utf8;
}

int ng_windows_set_text_content(NGHandle text_handle, const char* content) {
    if (!text_handle || !content) return NG_ERROR_INVALID_PARAMETER;
    HWND hwnd = (HWND)text_handle;
    SetPropA(hwnd, TEXT_QUIET_PROP, (HANDLE)1);
    int result = ng_windows_set_window_text(hwnd, content);
    RemovePropA(hwnd, TEXT_QUIET_PROP);
    return result;
}

char* ng_windows_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;
    return ng_windows_text_content((HWND)text_handle);
}

void ng_windows_free_text_content(char* content) {
    if (content) {
        free(content);
    }
}

static __declspec(thread) int g_old_rich_edit;

/* Lets tests take the path for Windows without msftedit.dll. */
void ng_windows_force_old_rich_edit(int on) {
    g_old_rich_edit = on;
}

HWND ng_windows_create_rich_text(int editable) {
    int modern = !g_old_rich_edit && LoadLibraryW(L"msftedit.dll") != NULL;
    if (!modern && !LoadLibraryW(L"riched20.dll")) return NULL;
    const wchar_t* cls = modern ? MSFTEDIT_CLASS : RICHEDIT_CLASSW;
    DWORD style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | WS_HSCROLL | ES_MULTILINE |
                  ES_AUTOVSCROLL | ES_AUTOHSCROLL | ES_WANTRETURN;
    if (!editable) style |= ES_READONLY;
    HWND edit = ng_windows_create_control(WS_EX_CLIENTEDGE, cls, "", style,
                                          400, 100, ng_windows_detached_parent(), 0);
    if (edit) ng_windows_unlimit_text(edit);
    return edit;
}

NGHandle ng_windows_create_text_field(void) {
    return (NGHandle)ng_windows_create_control(
        WS_EX_CLIENTEDGE, L"EDIT", "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL,
        200, 24, ng_windows_detached_parent(), 0);
}

int ng_windows_text_unit(void) {
    return NG_TEXT_UNIT_UTF16;
}

int ng_windows_text_get_selection(NGHandle text, int* start, int* end) {
    HWND hwnd = (HWND)text;
    if (!is_text(hwnd) || !start || !end) return NG_ERROR_INVALID_HANDLE;
    if (is_rich(hwnd)) {
        CHARRANGE range = {0, 0};
        SendMessageW(hwnd, EM_EXGETSEL, 0, (LPARAM)&range);
        *start = range.cpMin;
        *end = range.cpMax;
    } else {
        DWORD from = 0, to = 0;
        SendMessageW(hwnd, EM_GETSEL, (WPARAM)&from, (LPARAM)&to);
        *start = (int)from;
        *end = (int)to;
    }
    return NG_SUCCESS;
}

int ng_windows_text_set_selection(NGHandle text, int start, int end) {
    HWND hwnd = (HWND)text;
    if (!is_text(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (start < 0 || end < start) return NG_ERROR_INVALID_PARAMETER;
    if (is_rich(hwnd)) {
        CHARRANGE range = {start, end};
        SendMessageW(hwnd, EM_EXSETSEL, 0, (LPARAM)&range);
    } else {
        SendMessageW(hwnd, EM_SETSEL, (WPARAM)start, (LPARAM)end);
    }
    return NG_SUCCESS;
}

int ng_windows_text_replace(NGHandle text, int start, int end, const char* replacement) {
    HWND hwnd = (HWND)text;
    if (!is_text(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (!replacement) return NG_ERROR_INVALID_PARAMETER;
    wchar_t* wide = ng_windows_utf8_to_wide(replacement);
    if (!wide) return NG_ERROR_INVALID_PARAMETER;
    int result = ng_windows_text_set_selection(text, start, end);
    if (result == NG_SUCCESS) {
        SetPropA(hwnd, TEXT_QUIET_PROP, (HANDLE)1);
        SendMessageW(hwnd, EM_REPLACESEL, TRUE, (LPARAM)wide);
        RemovePropA(hwnd, TEXT_QUIET_PROP);
    }
    free(wide);
    return result;
}

int ng_windows_text_undo(NGHandle text, int redo) {
    HWND hwnd = (HWND)text;
    if (!is_text(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (redo && !is_rich(hwnd)) return NG_ERROR_UNSUPPORTED;
    if (!ng_windows_text_can_undo(text, redo)) return NG_ERROR_INVALID_PARAMETER;
    SetPropA(hwnd, TEXT_QUIET_PROP, (HANDLE)1);
    SendMessageW(hwnd, redo ? EM_REDO : EM_UNDO, 0, 0);
    RemovePropA(hwnd, TEXT_QUIET_PROP);
    return NG_SUCCESS;
}

int ng_windows_text_can_undo(NGHandle text, int redo) {
    HWND hwnd = (HWND)text;
    if (!is_text(hwnd)) return 0;
    if (redo) return is_rich(hwnd) && SendMessageW(hwnd, EM_CANREDO, 0, 0) ? 1 : 0;
    return SendMessageW(hwnd, EM_CANUNDO, 0, 0) ? 1 : 0;
}

int ng_windows_text_set_feature(NGHandle text, int feature, int on) {
    HWND hwnd = (HWND)text;
    if (!is_text(hwnd)) return NG_ERROR_INVALID_HANDLE;
    switch (feature) {
        case NG_TEXT_FEATURE_READ_ONLY:
            SendMessageW(hwnd, EM_SETREADONLY, on ? TRUE : FALSE, 0);
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_WRAP:
            if (!is_rich(hwnd)) return NG_ERROR_UNSUPPORTED;
            if (on) RemovePropA(hwnd, TEXT_NO_WRAP_PROP);
            else SetPropA(hwnd, TEXT_NO_WRAP_PROP, (HANDLE)1);
            SendMessageW(hwnd, EM_SETTARGETDEVICE, 0, on ? 0 : 1);
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_MONOSPACE:
            SendMessageW(hwnd, WM_SETFONT,
                         (WPARAM)(on ? ng_windows_mono_font(hwnd) : ng_windows_ui_font(hwnd)), TRUE);
            ng_windows_relayout(hwnd);
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_UNDO:
            if (!is_rich(hwnd)) return NG_ERROR_UNSUPPORTED;
            SendMessageW(hwnd, EM_SETUNDOLIMIT, on ? 100 : 0, 0);
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_SECRET:
            if (is_rich(hwnd) || (GetWindowLongPtrW(hwnd, GWL_STYLE) & ES_MULTILINE)) return NG_ERROR_UNSUPPORTED;
            SendMessageW(hwnd, EM_SETPASSWORDCHAR, on ? 0x25CF : 0, 0);
            InvalidateRect(hwnd, NULL, TRUE);
            return NG_SUCCESS;
        default:
            return NG_ERROR_INVALID_PARAMETER;
    }
}
