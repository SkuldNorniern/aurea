/* Input method (IMM32) support: composed text goes to the application as
 * preedit, committed text as text input, and the candidate window follows the
 * caret area the application reports. Aurea draws the composition itself, so
 * the IME's own composition window is turned off. */

#include "ime.h"
#include "window.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <imm.h>
#include <stdlib.h>

#define AUREA_IME_X_PROP "AureaImeX"
#define AUREA_IME_Y_PROP "AureaImeY"
#define AUREA_IME_W_PROP "AureaImeW"
#define AUREA_IME_H_PROP "AureaImeH"
#define AUREA_IME_SET_PROP "AureaImeSet"

static int ng_windows_ime_physical(HWND hwnd, float logical) {
    float scale = ng_windows_get_scale_factor((NGHandle)hwnd);
    if (scale <= 0.0f) scale = 1.0f;
    return (int)(logical * scale + 0.5f);
}

static int ng_windows_ime_prop(HWND hwnd, const char* name) {
    return (int)(INT_PTR)GetPropA(hwnd, name);
}

/* Moves the composition point and the candidate window to the stored caret
 * area. Nothing to do until the application has reported one. */
static void ng_windows_ime_place(HWND hwnd) {
    if (!GetPropA(hwnd, AUREA_IME_SET_PROP)) return;
    HIMC imc = ImmGetContext(hwnd);
    if (!imc) return;

    int x = ng_windows_ime_prop(hwnd, AUREA_IME_X_PROP);
    int y = ng_windows_ime_prop(hwnd, AUREA_IME_Y_PROP);
    int w = ng_windows_ime_prop(hwnd, AUREA_IME_W_PROP);
    int h = ng_windows_ime_prop(hwnd, AUREA_IME_H_PROP);

    COMPOSITIONFORM composition = {0};
    composition.dwStyle = CFS_POINT;
    composition.ptCurrentPos.x = x;
    composition.ptCurrentPos.y = y;
    ImmSetCompositionWindow(imc, &composition);

    /* CFS_EXCLUDE keeps the candidate list off the caret area itself. */
    CANDIDATEFORM candidate = {0};
    candidate.dwIndex = 0;
    candidate.dwStyle = CFS_EXCLUDE;
    candidate.ptCurrentPos.x = x;
    candidate.ptCurrentPos.y = y + h;
    candidate.rcArea.left = x;
    candidate.rcArea.top = y;
    candidate.rcArea.right = x + w;
    candidate.rcArea.bottom = y + h;
    ImmSetCandidateWindow(imc, &candidate);

    ImmReleaseContext(hwnd, imc);
}

int ng_windows_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    HWND hwnd = (HWND)window;
    SetPropA(hwnd, AUREA_IME_X_PROP, (HANDLE)(INT_PTR)ng_windows_ime_physical(hwnd, x));
    SetPropA(hwnd, AUREA_IME_Y_PROP, (HANDLE)(INT_PTR)ng_windows_ime_physical(hwnd, y));
    SetPropA(hwnd, AUREA_IME_W_PROP, (HANDLE)(INT_PTR)ng_windows_ime_physical(hwnd, width));
    SetPropA(hwnd, AUREA_IME_H_PROP, (HANDLE)(INT_PTR)ng_windows_ime_physical(hwnd, height));
    SetPropA(hwnd, AUREA_IME_SET_PROP, (HANDLE)1);
    ng_windows_ime_place(hwnd);
    return NG_SUCCESS;
}

/* One composition string (GCS_COMPSTR or GCS_RESULTSTR) as UTF-8, or NULL.
 * `units` receives its length in UTF-16 code units. Free with free(). */
static char* ng_windows_ime_string(HIMC imc, DWORD kind, LONG* units) {
    LONG bytes = ImmGetCompositionStringW(imc, kind, NULL, 0);
    if (units) *units = 0;
    if (bytes < 0) return NULL;

    LONG count = bytes / (LONG)sizeof(wchar_t);
    wchar_t* wide = (wchar_t*)malloc((size_t)bytes + sizeof(wchar_t));
    if (!wide) return NULL;
    if (bytes > 0) ImmGetCompositionStringW(imc, kind, wide, (DWORD)bytes);
    wide[count] = L'\0';

    int len = WideCharToMultiByte(CP_UTF8, 0, wide, (int)count, NULL, 0, NULL, NULL);
    char* utf8 = (char*)malloc((size_t)len + 1);
    if (utf8) {
        if (len > 0) WideCharToMultiByte(CP_UTF8, 0, wide, (int)count, utf8, len, NULL, NULL);
        utf8[len] = '\0';
    }
    if (units) *units = count;
    free(wide);
    return utf8;
}

/* The IME caret, converted from UTF-16 units to a UTF-8 byte offset. */
static int ng_windows_ime_cursor(HIMC imc) {
    LONG cursor = ImmGetCompositionStringW(imc, GCS_CURSORPOS, NULL, 0);
    if (cursor < 0) return -1;

    LONG bytes = ImmGetCompositionStringW(imc, GCS_COMPSTR, NULL, 0);
    if (bytes <= 0) return 0;
    LONG count = bytes / (LONG)sizeof(wchar_t);
    if (cursor > count) cursor = count;

    wchar_t* wide = (wchar_t*)malloc((size_t)bytes);
    if (!wide) return -1;
    ImmGetCompositionStringW(imc, GCS_COMPSTR, wide, (DWORD)bytes);
    int offset = WideCharToMultiByte(CP_UTF8, 0, wide, (int)cursor, NULL, 0, NULL, NULL);
    free(wide);
    return offset;
}

static void ng_windows_ime_composition(HWND hwnd, LPARAM flags) {
    HIMC imc = ImmGetContext(hwnd);
    if (!imc) return;

    if (flags & GCS_RESULTSTR) {
        char* result = ng_windows_ime_string(imc, GCS_RESULTSTR, NULL);
        if (result && result[0] != '\0') ng_invoke_text_input((void*)hwnd, result);
        free(result);
    }

    if (flags & GCS_COMPSTR) {
        char* composing = ng_windows_ime_string(imc, GCS_COMPSTR, NULL);
        int cursor = (flags & GCS_CURSORPOS) ? ng_windows_ime_cursor(imc) : -1;
        ng_invoke_ime_preedit((void*)hwnd, composing ? composing : "", cursor);
        free(composing);
    } else if (flags & GCS_RESULTSTR) {
        /* Committed with nothing new being composed. */
        ng_invoke_ime_preedit((void*)hwnd, "", -1);
    }

    ImmReleaseContext(hwnd, imc);
}

int ng_windows_ime_message(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam, LRESULT* result) {
    switch (msg) {
        case WM_IME_SETCONTEXT:
            /* Aurea draws the composition; keep only the candidate list. */
            if (wParam) lParam &= ~ISC_SHOWUICOMPOSITIONWINDOW;
            *result = DefWindowProcW(hwnd, msg, wParam, lParam);
            return 1;
        case WM_IME_STARTCOMPOSITION:
            ng_windows_ime_place(hwnd);
            *result = 0;
            return 1;
        case WM_IME_COMPOSITION:
            /* Handled here rather than by DefWindowProc, which would turn the
               result into WM_IME_CHAR/WM_CHAR and type it a second time. */
            ng_windows_ime_composition(hwnd, lParam);
            ng_windows_ime_place(hwnd);
            *result = 0;
            return 1;
        case WM_IME_ENDCOMPOSITION:
            ng_invoke_ime_preedit((void*)hwnd, "", -1);
            *result = 0;
            return 1;
        default:
            return 0;
    }
}

void ng_windows_ime_release(HWND hwnd) {
    RemovePropA(hwnd, AUREA_IME_X_PROP);
    RemovePropA(hwnd, AUREA_IME_Y_PROP);
    RemovePropA(hwnd, AUREA_IME_W_PROP);
    RemovePropA(hwnd, AUREA_IME_H_PROP);
    RemovePropA(hwnd, AUREA_IME_SET_PROP);
}
