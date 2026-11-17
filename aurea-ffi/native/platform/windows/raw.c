#include "raw.h"
#include "window_state.h"
#include "common/rust_callbacks.h"

/* The window that has raw mouse input registered. */
static HWND g_raw_holder = NULL;

static void ng_windows_set_raw_mouse(HWND target) {
    RAWINPUTDEVICE rid;
    rid.usUsagePage = 0x01;
    rid.usUsage = 0x02;
    rid.dwFlags = target ? 0 : RIDEV_REMOVE;
    rid.hwndTarget = target;
    if (RegisterRawInputDevices(&rid, 1, sizeof(rid))) g_raw_holder = target;
}

void ng_windows_raw_mouse_take(HWND hwnd, int raw) {
    if (raw) {
        if (g_raw_holder != hwnd) ng_windows_set_raw_mouse(hwnd);
    } else if (g_raw_holder == hwnd) {
        ng_windows_set_raw_mouse(NULL);
    }
}

void ng_windows_raw_input(HWND hwnd, LPARAM lParam) {
    NGWindow* w = ng_windows_window_state(hwnd);
    if (!w || w->grab_mode != 2) return;
    RAWINPUT raw;
    UINT size = sizeof(raw);
    if (GetRawInputData((HRAWINPUT)lParam, RID_INPUT, &raw, &size, sizeof(RAWINPUTHEADER)) == size
        && raw.header.dwType == RIM_TYPEMOUSE) {
        ng_invoke_raw_mouse_motion(
            (void*)hwnd,
            (double)raw.data.mouse.lLastX,
            (double)raw.data.mouse.lLastY);
    }
}
