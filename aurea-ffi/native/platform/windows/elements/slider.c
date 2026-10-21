#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include <windows.h>
#include <commctrl.h>
#include <math.h>
#include <stdlib.h>


#define SLIDER_STEPS 10000
#define SLIDER_SUBCLASS_ID 1

typedef struct {
    double min;
    double max;
} SliderRange;

static LRESULT CALLBACK slider_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                    UINT_PTR id, DWORD_PTR data) {
    if (msg == WM_NCDESTROY) {
        ng_windows_remove_subclass(hwnd, slider_proc, id);
        free((SliderRange*)data);
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

static SliderRange* slider_range(HWND hwnd) {
    DWORD_PTR data = 0;
    return ng_windows_get_subclass(hwnd, slider_proc, SLIDER_SUBCLASS_ID, &data) ? (SliderRange*)data : NULL;
}

NGHandle ng_windows_create_slider(double min, double max) {
    if (!(min < max) || !(min > -HUGE_VAL) || !(max < HUGE_VAL)) return NULL;

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

    SliderRange* range = (SliderRange*)malloc(sizeof(SliderRange));
    if (!range || !ng_windows_set_subclass(hwnd, slider_proc, SLIDER_SUBCLASS_ID, (DWORD_PTR)range)) {
        free(range);
        DestroyWindow(hwnd);
        return NULL;
    }
    range->min = min;
    range->max = max;
    SendMessageW(hwnd, TBM_SETRANGEMIN, FALSE, 0);
    SendMessageW(hwnd, TBM_SETRANGEMAX, FALSE, SLIDER_STEPS);
    SendMessageW(hwnd, TBM_SETLINESIZE, 0, SLIDER_STEPS / 100);
    SendMessageW(hwnd, TBM_SETPAGESIZE, 0, SLIDER_STEPS / 10);
    SendMessageW(hwnd, TBM_SETPOS, TRUE, SLIDER_STEPS / 2);
    return (NGHandle)hwnd;
}

int ng_windows_slider_set_value(NGHandle slider, double value) {
    SliderRange* range = slider ? slider_range((HWND)slider) : NULL;
    if (!range) return NG_ERROR_INVALID_HANDLE;
    double t = (value - range->min) / (range->max - range->min);
    if (!(t > 0.0)) t = 0.0;
    if (t > 1.0) t = 1.0;
    SendMessageW((HWND)slider, TBM_SETPOS, TRUE, (LPARAM)(t * SLIDER_STEPS + 0.5));
    return NG_SUCCESS;
}

double ng_windows_slider_get_value(NGHandle slider) {
    SliderRange* range = slider ? slider_range((HWND)slider) : NULL;
    if (!range) return 0.0;
    double t = (double)SendMessageW((HWND)slider, TBM_GETPOS, 0, 0) / SLIDER_STEPS;
    return range->min + t * (range->max - range->min);
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
