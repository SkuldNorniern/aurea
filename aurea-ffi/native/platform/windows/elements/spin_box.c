#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include <windows.h>
#include <commctrl.h>
#include <stdio.h>
#include <stdlib.h>

/* An edit control with the up-down inside it, so layout and text see one window. */

#define SPIN_SUBCLASS_ID 0x5350
#define SPIN_PROP "AureaSpinBox"
/* The arrows stay put; each press is read and put back. */
#define SPIN_ARROW_REST 50

typedef struct {
    double min;
    double max;
    double step;
    double value;
    HWND arrows;
} SpinBox;

static SpinBox* spin_of(HWND hwnd) {
    return hwnd && IsWindow(hwnd) ? (SpinBox*)GetPropA(hwnd, SPIN_PROP) : NULL;
}

static double spin_clamp(const SpinBox* spin, double value) {
    return value < spin->min ? spin->min : value > spin->max ? spin->max : value;
}

static double spin_read(HWND hwnd, const SpinBox* spin) {
    char text[64];
    GetWindowTextA(hwnd, text, sizeof(text));
    char* end = NULL;
    double value = strtod(text, &end);
    return end == text ? spin->value : spin_clamp(spin, value);
}

/* Shows `value`; quiet when the app set it. */
static void spin_show(HWND hwnd, SpinBox* spin, double value, int quiet) {
    spin->value = spin_clamp(spin, value);
    char text[64];
    snprintf(text, sizeof(text), "%g", spin->value);
    if (quiet) SetPropA(hwnd, TEXT_QUIET_PROP, (HANDLE)1);
    SetWindowTextA(hwnd, text);
    if (quiet) RemovePropA(hwnd, TEXT_QUIET_PROP);
}

static void spin_step(HWND hwnd, SpinBox* spin, int steps) {
    spin_show(hwnd, spin, spin_read(hwnd, spin) + steps * spin->step, 0);
}

static void spin_fit_arrows(HWND hwnd, const SpinBox* spin) {
    RECT client;
    GetClientRect(hwnd, &client);
    int width = GetSystemMetrics(SM_CXVSCROLL);
    MoveWindow(spin->arrows, client.right - width, 0, width, client.bottom, TRUE);
    SendMessageW(hwnd, EM_SETMARGINS, EC_RIGHTMARGIN, MAKELPARAM(0, width + 2));
}

static LRESULT CALLBACK spin_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam, UINT_PTR id, DWORD_PTR data) {
    (void)id;
    SpinBox* spin = (SpinBox*)data;
    switch (msg) {
        case WM_NOTIFY:
            if (((LPNMHDR)lParam)->code == UDN_DELTAPOS) {
                spin_step(hwnd, spin, ((LPNMUPDOWN)lParam)->iDelta > 0 ? 1 : -1);
                return 1;
            }
            break;
        case WM_KEYDOWN:
            if (wParam == VK_UP || wParam == VK_DOWN) {
                spin_step(hwnd, spin, wParam == VK_UP ? 1 : -1);
                return 0;
            }
            break;
        case WM_KILLFOCUS: {
            double value = spin_read(hwnd, spin);
            spin_show(hwnd, spin, value, value == spin->value);
            break;
        }
        case WM_SIZE:
            spin_fit_arrows(hwnd, spin);
            break;
        case WM_ENABLE:
            EnableWindow(spin->arrows, (BOOL)wParam);
            break;
        case WM_NCDESTROY:
            RemovePropA(hwnd, SPIN_PROP);
            ng_windows_remove_subclass(hwnd, spin_proc, SPIN_SUBCLASS_ID);
            free(spin);
            break;
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_spin_box(double min, double max, double step, double value) {
    ng_windows_init_common_controls(ICC_UPDOWN_CLASS);
    SpinBox* spin = (SpinBox*)calloc(1, sizeof(SpinBox));
    if (!spin) return NULL;
    HWND edit = ng_windows_create_control(WS_EX_CLIENTEDGE, L"EDIT", "",
                                          WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL, 120, 24,
                                          ng_windows_detached_parent(), 0);
    if (!edit) {
        free(spin);
        return NULL;
    }
    spin->min = min;
    spin->max = max < min ? min : max;
    spin->step = step > 0.0 ? step : 1.0;
    spin->arrows = CreateWindowExW(0, UPDOWN_CLASSW, NULL, WS_CHILD | WS_VISIBLE, 0, 0, 0, 0, edit, NULL,
                                   GetModuleHandleW(NULL), NULL);
    if (!spin->arrows || !ng_windows_set_subclass(edit, spin_proc, SPIN_SUBCLASS_ID, (DWORD_PTR)spin)) {
        DestroyWindow(edit);
        free(spin);
        return NULL;
    }
    SendMessageW(spin->arrows, UDM_SETRANGE32, 0, SPIN_ARROW_REST * 2);
    SendMessageW(spin->arrows, UDM_SETPOS32, 0, SPIN_ARROW_REST);
    SetPropA(edit, SPIN_PROP, spin);
    spin_show(edit, spin, value, 1);
    spin_fit_arrows(edit, spin);
    return (NGHandle)edit;
}

double ng_windows_spin_box_get_value(NGHandle spin_box) {
    HWND hwnd = (HWND)spin_box;
    SpinBox* spin = spin_of(hwnd);
    return spin ? spin_read(hwnd, spin) : 0.0;
}

int ng_windows_spin_box_set_value(NGHandle spin_box, double value) {
    HWND hwnd = (HWND)spin_box;
    SpinBox* spin = spin_of(hwnd);
    if (!spin) return NG_ERROR_INVALID_HANDLE;
    spin_show(hwnd, spin, value, 1);
    return NG_SUCCESS;
}
