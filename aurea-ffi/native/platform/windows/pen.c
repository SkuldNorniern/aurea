
#include "pen.h"
#include "utils.h"
#include "window.h"
#include "common/rust_callbacks.h"

#define AUREA_PEN_HISTORY 64

/* How long Windows Ink stays the pen's source after it last sent one, in Auto. */
#define AUREA_PEN_INK_LIVE_MS 1000

static UINT32 g_pen_last_id;
static UINT32 g_pen_last_frame;
static int g_pen_reported;
static int g_pen_mode = AUREA_PEN_MODE_AUTO;
static ULONGLONG g_ink_last_tick;
static int g_ink_seen;
static int g_ink_contact;
static UINT32 g_ink_contact_id;
static double g_ink_last_x;
static double g_ink_last_y;
static unsigned int g_ink_last_flags;
static HWND g_ink_root;

int ng_windows_pen_mode(void) {
    return g_pen_mode;
}

static void ng_windows_ink_drop(void) {
    if (!g_ink_contact) return;
    g_ink_contact = 0;
    ng_invoke_pen((void*)g_ink_root, g_ink_last_x, g_ink_last_y, 0.0f, 0.0f, 0.0f, 0.0f, AUREA_PEN_CANCEL, g_ink_last_flags,
                  ng_windows_modifiers(), 0.0);
}

void ng_windows_pen_set_mode(int mode) {
    g_pen_mode = mode;
    g_ink_seen = 0;
    /* A stroke Ink was drawing has no more samples coming once Wintab takes over. */
    if (mode == AUREA_PEN_MODE_WINTAB) ng_windows_ink_drop();
    ng_windows_wintab_sync();
}

int ng_windows_pen_ink_live(void) {
    if (g_pen_mode != AUREA_PEN_MODE_AUTO) return 0;
    if (g_ink_contact) return 1;
    return g_ink_seen && GetTickCount64() - g_ink_last_tick < AUREA_PEN_INK_LIVE_MS;
}

static unsigned int ng_windows_pen_phase(POINTER_FLAGS flags) {
    if (flags & POINTER_FLAG_DOWN) return AUREA_PEN_DOWN;
    if (flags & POINTER_FLAG_UP) return AUREA_PEN_UP;
    if (flags & POINTER_FLAG_INCONTACT) return AUREA_PEN_MOVE;
    return AUREA_PEN_HOVER;
}

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

static double ng_windows_pen_age(const POINTER_INFO* info) {
    if (info->PerformanceCount == 0) return 0.0;
    LARGE_INTEGER now, frequency;
    if (!QueryPerformanceCounter(&now) || !QueryPerformanceFrequency(&frequency) || frequency.QuadPart == 0) {
        return 0.0;
    }
    return (double)(now.QuadPart - (LONGLONG)info->PerformanceCount) / (double)frequency.QuadPart;
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
    unsigned int phase = ng_windows_pen_phase(pen->pointerInfo.pointerFlags);
    g_ink_contact = phase == AUREA_PEN_DOWN || phase == AUREA_PEN_MOVE;
    g_ink_contact_id = pen->pointerInfo.pointerId;
    g_ink_last_x = (sx - origin.x) / scale;
    g_ink_last_y = (sy - origin.y) / scale;
    g_ink_last_flags = flags;
    g_ink_root = root;
    ng_invoke_pen(
        (void*)root,
        g_ink_last_x,
        g_ink_last_y,
        ng_windows_pen_pressure(pen),
        tilt_x,
        tilt_y,
        twist,
        phase,
        flags,
        modifiers,
        ng_windows_pen_age(&pen->pointerInfo));
}

typedef BOOL (WINAPI *PointerTypeFunc)(UINT32, POINTER_INPUT_TYPE*);
typedef BOOL (WINAPI *PenInfoFunc)(UINT32, POINTER_PEN_INFO*);
typedef BOOL (WINAPI *PenHistoryFunc)(UINT32, UINT32*, POINTER_PEN_INFO*);
typedef BOOL (WINAPI *DeviceRectsFunc)(HANDLE, RECT*, RECT*);

/* Windows 8 and later; pointer messages never come before that. */
static struct {
    int looked_up;
    PointerTypeFunc type;
    PenInfoFunc info;
    PenHistoryFunc history;
    DeviceRectsFunc rects;
} g_pointer;

static int ng_windows_pointer_api(void) {
    if (!g_pointer.looked_up) {
        g_pointer.looked_up = 1;
        g_pointer.type = (PointerTypeFunc)ng_windows_api("user32.dll", "GetPointerType");
        g_pointer.info = (PenInfoFunc)ng_windows_api("user32.dll", "GetPointerPenInfo");
        g_pointer.history = (PenHistoryFunc)ng_windows_api("user32.dll", "GetPointerPenInfoHistory");
        g_pointer.rects = (DeviceRectsFunc)ng_windows_api("user32.dll", "GetPointerDeviceRects");
    }
    return g_pointer.type && g_pointer.info && g_pointer.history && g_pointer.rects;
}

void ng_windows_pen_message(HWND root, UINT msg, WPARAM wParam) {
    if (!root || (msg != WM_POINTERDOWN && msg != WM_POINTERUPDATE && msg != WM_POINTERUP)) return;
    if (!ng_windows_pointer_api()) return;
    UINT32 id = GET_POINTERID_WPARAM(wParam);
    POINTER_INPUT_TYPE type;
    if (!g_pointer.type(id, &type) || type != PT_PEN) return;
    if (g_pen_mode == AUREA_PEN_MODE_WINTAB) return;
    g_ink_seen = 1;
    g_ink_last_tick = GetTickCount64();
    ng_windows_wintab_yield();

    POINTER_PEN_INFO samples[AUREA_PEN_HISTORY];
    UINT32 count = AUREA_PEN_HISTORY;
    if (!g_pointer.history(id, &count, samples) || count == 0) {
        count = 1;
        if (!g_pointer.info(id, &samples[0])) return;
    }
    if (count > AUREA_PEN_HISTORY) count = AUREA_PEN_HISTORY;
    UINT32 frame = samples[0].pointerInfo.frameId;
    if (g_pen_reported && g_pen_last_id == id && g_pen_last_frame == frame) return;
    g_pen_reported = 1;
    g_pen_last_id = id;
    g_pen_last_frame = frame;

    RECT device, display;
    int mapped = g_pointer.rects(samples[0].pointerInfo.sourceDevice, &device, &display) &&
                 device.right > device.left && device.bottom > device.top;
    POINT origin = {0, 0};
    ClientToScreen(root, &origin);
    float scale = ng_windows_get_scale_factor((NGHandle)root);
    double logical = scale > 1.0f ? (double)scale : 1.0;
    unsigned int modifiers = ng_windows_modifiers();

    for (UINT32 i = count; i-- > 0;) {
        ng_windows_pen_emit(root, &samples[i], &device, &display, mapped, origin, logical, modifiers);
    }
}

void ng_windows_pen_capture_lost(HWND root, WPARAM wParam) {
    if (root && g_ink_contact_id == GET_POINTERID_WPARAM(wParam)) ng_windows_ink_drop();
}
