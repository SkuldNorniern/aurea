#include "gesture.h"
#include "utils.h"
#include "common/input.h"
#include "common/rust_callbacks.h"

/* Zoom and rotate report what changed since the last message, so each keeps the last value. */
static ULONGLONG g_zoom_span;
static double g_turned;

/* Windows 7 and later; older systems never send WM_GESTURE. */
typedef BOOL (WINAPI *SetConfigFunc)(HWND, DWORD, UINT, PGESTURECONFIG, UINT);
typedef BOOL (WINAPI *GetInfoFunc)(HGESTUREINFO, PGESTUREINFO);
typedef BOOL (WINAPI *CloseInfoFunc)(HGESTUREINFO);

void ng_windows_enable_gestures(HWND hwnd) {
    static SetConfigFunc set_config;
    if (!set_config) set_config = (SetConfigFunc)ng_windows_api("user32.dll", "SetGestureConfig");
    if (!set_config) return;
    GESTURECONFIG config[2] = {
        {GID_ZOOM, GC_ZOOM, 0},
        {GID_ROTATE, GC_ROTATE, 0},
    };
    set_config(hwnd, 0, 2, config, sizeof(config[0]));
}

int ng_windows_gesture_message(HWND hwnd, LPARAM lParam) {
    static GetInfoFunc get_info;
    static CloseInfoFunc close_info;
    if (!get_info) {
        get_info = (GetInfoFunc)ng_windows_api("user32.dll", "GetGestureInfo");
        close_info = (CloseInfoFunc)ng_windows_api("user32.dll", "CloseGestureInfoHandle");
    }
    if (!get_info || !close_info) return 0;

    GESTUREINFO info;
    ZeroMemory(&info, sizeof(info));
    info.cbSize = sizeof(info);
    if (!get_info((HGESTUREINFO)lParam, &info)) return 0;
    if (info.dwID != GID_ZOOM && info.dwID != GID_ROTATE) return 0;

    POINT at = {info.ptsLocation.x, info.ptsLocation.y};
    ScreenToClient(hwnd, &at);
    double x = ng_windows_logical(hwnd, at.x);
    double y = ng_windows_logical(hwnd, at.y);
    int begin = (info.dwFlags & GF_BEGIN) != 0;

    if (info.dwID == GID_ZOOM) {
        ULONGLONG span = info.ullArguments;
        if (begin || g_zoom_span == 0) {
            g_zoom_span = span;
        } else if (span > 0 && span != g_zoom_span) {
            double factor = (double)span / (double)g_zoom_span;
            g_zoom_span = span;
            ng_invoke_gesture((void*)hwnd, NG_GESTURE_MAGNIFY, factor, x, y);
        }
    } else {
        /* Angle since the gesture began; sign flipped to Rotate's clockwise. Check on a touchscreen. */
        double turned = GID_ROTATE_ANGLE_FROM_ARGUMENT(info.ullArguments);
        double delta = begin ? 0.0 : turned - g_turned;
        g_turned = turned;
        if (delta != 0.0) ng_invoke_gesture((void*)hwnd, NG_GESTURE_ROTATE, -delta, x, y);
    }

    close_info((HGESTUREINFO)lParam);
    return 1;
}
