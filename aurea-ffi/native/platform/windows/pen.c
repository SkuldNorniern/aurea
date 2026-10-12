/* Pen input from the pointer messages: pressure, tilt, twist, the eraser and
 * the barrel button, with the samples Windows gathered between two messages,
 * which a fast stroke would lose otherwise. */

#include "pen.h"
#include "utils.h"
#include "window.h"
#include "common/rust_callbacks.h"

/* Samples kept from one message; a pen at 200 Hz fills a few per frame. */
#define AUREA_PEN_HISTORY 64

#define AUREA_PEN_HOVER 0
#define AUREA_PEN_DOWN 1
#define AUREA_PEN_MOVE 2
#define AUREA_PEN_UP 3

#define AUREA_PEN_ERASER 1u
#define AUREA_PEN_BARREL 2u

/* A canvas reports its pen and then the message may bubble up to the window,
 * so a frame already reported is skipped. */
static UINT32 g_pen_last_id;
static UINT32 g_pen_last_frame;
static int g_pen_reported;

static unsigned int ng_windows_pen_phase(POINTER_FLAGS flags) {
    if (flags & POINTER_FLAG_DOWN) return AUREA_PEN_DOWN;
    if (flags & POINTER_FLAG_UP) return AUREA_PEN_UP;
    if (flags & POINTER_FLAG_INCONTACT) return AUREA_PEN_MOVE;
    return AUREA_PEN_HOVER;
}

/* Screen position in physical pixels. The HIMETRIC one is finer than the
 * pixel one; mapped onto the screen when the device says how. */
static void ng_windows_pen_screen(const POINTER_INFO* info, const RECT* device, const RECT* display, int mapped,
                                  double* x, double* y) {
    if (!mapped) {
        *x = (double)info->ptPixelLocation.x;
        *y = (double)info->ptPixelLocation.y;
        return;
    }
    double dw = (double)(device->right - device->left);
    double dh = (double)(device->bottom - device->top);
    *x = display->left + (info->ptHimetricLocation.x - device->left) * (display->right - display->left) / dw;
    *y = display->top + (info->ptHimetricLocation.y - device->top) * (display->bottom - display->top) / dh;
}

static float ng_windows_pen_pressure(const POINTER_PEN_INFO* pen) {
    if (pen->penMask & PEN_MASK_PRESSURE) return (float)pen->pressure / 1024.0f;
    return (pen->pointerInfo.pointerFlags & POINTER_FLAG_INCONTACT) ? 1.0f : 0.0f;
}

static void ng_windows_pen_emit(HWND root, const POINTER_PEN_INFO* pen, const RECT* device, const RECT* display,
                                int mapped, POINT origin, double scale, unsigned int modifiers) {
    double sx, sy;
    ng_windows_pen_screen(&pen->pointerInfo, device, display, mapped, &sx, &sy);
    float tilt_x = (pen->penMask & PEN_MASK_TILT_X) ? (float)pen->tiltX : 0.0f;
    float tilt_y = (pen->penMask & PEN_MASK_TILT_Y) ? (float)pen->tiltY : 0.0f;
    float twist = (pen->penMask & PEN_MASK_ROTATION) ? (float)pen->rotation : 0.0f;
    unsigned int flags = 0;
    if (pen->penFlags & (PEN_FLAG_ERASER | PEN_FLAG_INVERTED)) flags |= AUREA_PEN_ERASER;
    if (pen->penFlags & PEN_FLAG_BARREL) flags |= AUREA_PEN_BARREL;
    ng_invoke_pen(
        (void*)root,
        (sx - origin.x) / scale,
        (sy - origin.y) / scale,
        ng_windows_pen_pressure(pen),
        tilt_x,
        tilt_y,
        twist,
        ng_windows_pen_phase(pen->pointerInfo.pointerFlags),
        flags,
        modifiers);
}

void ng_windows_pen_message(HWND root, UINT msg, WPARAM wParam) {
    if (!root || (msg != WM_POINTERDOWN && msg != WM_POINTERUPDATE && msg != WM_POINTERUP)) return;
    UINT32 id = GET_POINTERID_WPARAM(wParam);
    POINTER_INPUT_TYPE type;
    if (!GetPointerType(id, &type) || type != PT_PEN) return;

    POINTER_PEN_INFO samples[AUREA_PEN_HISTORY];
    UINT32 count = AUREA_PEN_HISTORY;
    if (!GetPointerPenInfoHistory(id, &count, samples) || count == 0) {
        count = 1;
        if (!GetPointerPenInfo(id, &samples[0])) return;
    }
    if (count > AUREA_PEN_HISTORY) count = AUREA_PEN_HISTORY;
    UINT32 frame = samples[0].pointerInfo.frameId;
    if (g_pen_reported && g_pen_last_id == id && g_pen_last_frame == frame) return;
    g_pen_reported = 1;
    g_pen_last_id = id;
    g_pen_last_frame = frame;

    RECT device, display;
    int mapped = GetPointerDeviceRects(samples[0].pointerInfo.sourceDevice, &device, &display) &&
                 device.right > device.left && device.bottom > device.top;
    POINT origin = {0, 0};
    ClientToScreen(root, &origin);
    float scale = ng_windows_get_scale_factor((NGHandle)root);
    double logical = scale > 1.0f ? (double)scale : 1.0;
    unsigned int modifiers = ng_windows_modifiers();

    /* Newest first in the history; report them in the order they happened. */
    for (UINT32 i = count; i-- > 0;) {
        ng_windows_pen_emit(root, &samples[i], &device, &display, mapped, origin, logical, modifiers);
    }
}
