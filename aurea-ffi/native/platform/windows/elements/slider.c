#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include <windows.h>
#include <commctrl.h>
#include <math.h>
#include <stdlib.h>
#include <wchar.h>

#define SLIDER_STEPS 10000
#define SLIDER_SUBCLASS_ID 1

typedef struct {
    double min;
    double max;
} SliderState;

static LRESULT CALLBACK slider_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                    UINT_PTR id, DWORD_PTR data);

static SliderState* slider_state(HWND hwnd) {
    DWORD_PTR data = 0;
    return ng_windows_get_subclass(hwnd, slider_proc, SLIDER_SUBCLASS_ID, &data)
               ? (SliderState*)data
               : NULL;
}

static LRESULT CALLBACK slider_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                    UINT_PTR id, DWORD_PTR data) {
    if (msg == WM_NCDESTROY) {
        ng_windows_remove_subclass(hwnd, slider_proc, id);
        free((SliderState*)data);
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

/* Only as many decimals as one step of the trackbar can tell apart. */
static int slider_value_text(HWND hwnd, wchar_t* out, int len) {
    SliderState* s = slider_state(hwnd);
    if (!s) return 0;
    int digits = (int)ceil(-log10((s->max - s->min) / SLIDER_STEPS));
    if (digits < 0) digits = 0;
    if (digits > 9) digits = 9;
    int n = swprintf(out, (size_t)len, L"%.*f", digits, ng_windows_slider_get_value((NGHandle)hwnd));
    if (n <= 0) return 0;
    if (digits > 0) {
        while (out[n - 1] == L'0') out[--n] = 0;
        if (out[n - 1] == L'.') out[--n] = 0;
    }
    return 1;
}

NGHandle ng_windows_create_slider(double min, double max) {
    /* The span must be finite too, or the step size below is not. */
    if (!(min < max) || !(min > -HUGE_VAL) || !(max < HUGE_VAL) || !((max - min) < HUGE_VAL)) return NULL;

    HWND hwnd = CreateWindowExW(
        0,
        TRACKBAR_CLASSW,
        NULL,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | TBS_HORZ | TBS_NOTICKS,
        0, 0, 200, SLIDER_HEIGHT,
        ng_windows_detached_parent(),
        NULL,
        GetModuleHandleW(NULL),
        NULL
    );
    if (!hwnd) return NULL;

    SliderState* s = (SliderState*)malloc(sizeof(SliderState));
    if (!s) {
        DestroyWindow(hwnd);
        return NULL;
    }
    s->min = min;
    s->max = max;
    if (!ng_windows_set_subclass(hwnd, slider_proc, SLIDER_SUBCLASS_ID, (DWORD_PTR)s)) {
        free(s);
        DestroyWindow(hwnd);
        return NULL;
    }
    SendMessageW(hwnd, TBM_SETRANGEMIN, FALSE, 0);
    SendMessageW(hwnd, TBM_SETRANGEMAX, FALSE, SLIDER_STEPS);
    SendMessageW(hwnd, TBM_SETLINESIZE, 0, SLIDER_STEPS / 100);
    SendMessageW(hwnd, TBM_SETPAGESIZE, 0, SLIDER_STEPS / 10);
    SendMessageW(hwnd, TBM_SETPOS, TRUE, SLIDER_STEPS / 2);
    ng_windows_serve_value(hwnd, slider_value_text);
    return (NGHandle)hwnd;
}

int ng_windows_slider_set_value(NGHandle slider, double value) {
    SliderState* s = slider ? slider_state((HWND)slider) : NULL;
    if (!s) return NG_ERROR_INVALID_HANDLE;
    double t = (value - s->min) / (s->max - s->min);
    if (!(t > 0.0)) t = 0.0;
    if (t > 1.0) t = 1.0;
    SendMessageW((HWND)slider, TBM_SETPOS, TRUE, (LPARAM)(t * SLIDER_STEPS + 0.5));
    return NG_SUCCESS;
}

double ng_windows_slider_get_value(NGHandle slider) {
    SliderState* s = slider ? slider_state((HWND)slider) : NULL;
    if (!s) return 0.0;
    double t = (double)SendMessageW((HWND)slider, TBM_GETPOS, 0, 0) / SLIDER_STEPS;
    return s->min + t * (s->max - s->min);
}

int ng_windows_slider_set_enabled(NGHandle slider, int enabled) {
    if (!slider) return NG_ERROR_INVALID_HANDLE;
    EnableWindow((HWND)slider, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_windows_slider_invalidate(NGHandle slider) {
    if (!slider) return;
    InvalidateRect((HWND)slider, NULL, FALSE);
}
