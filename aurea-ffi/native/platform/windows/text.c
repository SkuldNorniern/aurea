#include "text.h"
#include "common/rust_callbacks.h"

void ng_windows_text_char(NGWindow* w, wchar_t wc) {
    HWND hwnd = w->hwnd;
    if (wc >= 0xD800 && wc <= 0xDBFF) {
        w->high_surrogate = wc;
        return;
    }

    wchar_t high = w->high_surrogate;
    w->high_surrogate = 0;
    wchar_t units[2];
    int count;
    if (high != 0 && wc >= 0xDC00 && wc <= 0xDFFF) {
        units[0] = high;
        units[1] = wc;
        count = 2;
    } else {
        units[0] = wc;
        count = 1;
    }

    char buffer[8];
    int len = WideCharToMultiByte(CP_UTF8, 0, units, count, buffer, (int)sizeof(buffer) - 1, NULL, NULL);
    if (len > 0) {
        buffer[len] = '\0';
        ng_invoke_text_input((void*)hwnd, buffer);
    }
}
