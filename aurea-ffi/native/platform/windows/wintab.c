#include "pen.h"
#include "window.h"
#include "utils.h"
#include "common/rust_callbacks.h"

#include <math.h>

/* wintab.h is not in the Windows SDK and the driver ships the dll, so only what is used is here. */

#define WT_DEFBASE 0x7FF0
#define WT_PACKET (WT_DEFBASE + 0)
#define WT_PROXIMITY (WT_DEFBASE + 5)

#define WTI_DEFSYSCTX 4
#define WTI_DEVICES 100
#define DVC_NPRESSURE 15
#define DVC_ORIENTATION 17

#define CXO_MESSAGES 0x0004

#define PK_STATUS 0x0002
#define PK_TIME 0x0004
#define PK_CURSOR 0x0020
#define PK_BUTTONS 0x0040
#define PK_X 0x0080
#define PK_Y 0x0100
#define PK_NORMAL_PRESSURE 0x0400
#define PK_ORIENTATION 0x1000
#define AUREA_WT_PACKET_DATA \
    (PK_STATUS | PK_TIME | PK_CURSOR | PK_BUTTONS | PK_X | PK_Y | PK_NORMAL_PRESSURE | PK_ORIENTATION)

#define TPS_PROXIMITY 0x0001
#define TPS_INVERT 0x0010

#define AUREA_WT_QUEUE 128
#define AUREA_WT_QUEUE_LEAST 16
#define AUREA_WT_NAME 40
#define AUREA_WT_PI 3.14159265358979323846

typedef DWORD WTPKT;
typedef DWORD FIX32;
typedef void* HCTX;

typedef struct {
    WCHAR lcName[AUREA_WT_NAME];
    UINT lcOptions;
    UINT lcStatus;
    UINT lcLocks;
    UINT lcMsgBase;
    UINT lcDevice;
    UINT lcPktRate;
    WTPKT lcPktData;
    WTPKT lcPktMode;
    WTPKT lcMoveMask;
    DWORD lcBtnDnMask;
    DWORD lcBtnUpMask;
    LONG lcInOrgX, lcInOrgY, lcInOrgZ;
    LONG lcInExtX, lcInExtY, lcInExtZ;
    LONG lcOutOrgX, lcOutOrgY, lcOutOrgZ;
    LONG lcOutExtX, lcOutExtY, lcOutExtZ;
    FIX32 lcSensX, lcSensY, lcSensZ;
    BOOL lcSysMode;
    int lcSysOrgX, lcSysOrgY;
    int lcSysExtX, lcSysExtY;
    FIX32 lcSysSensX, lcSysSensY;
} WtContext;

typedef struct {
    LONG axMin;
    LONG axMax;
    UINT axUnits;
    FIX32 axResolution;
} WtAxis;

/* Fields in the order of their PK_ bits, as AUREA_WT_PACKET_DATA asks. */
typedef struct {
    UINT status;
    DWORD time;
    UINT cursor;
    DWORD buttons;
    LONG x;
    LONG y;
    UINT pressure;
    int azimuth;
    int altitude;
    int twist;
} WtPacket;

typedef UINT (WINAPI *WtInfoFunc)(UINT, UINT, LPVOID);
typedef HCTX (WINAPI *WtOpenFunc)(HWND, WtContext*, BOOL);
typedef BOOL (WINAPI *WtCloseFunc)(HCTX);
typedef int (WINAPI *WtPacketsGetFunc)(HCTX, int, LPVOID);
typedef BOOL (WINAPI *WtQueueSizeSetFunc)(HCTX, int);

static struct {
    int looked_up;
    WtInfoFunc info;
    WtOpenFunc open;
    WtCloseFunc close;
    WtPacketsGetFunc packets_get;
    WtQueueSizeSetFunc queue_size_set;
} g_wt_api;

static struct {
    HWND active_root;
    HWND owner;
    HCTX context;
    WtContext shape;
    WtAxis pressure;
    WtAxis orientation[3];
    int contact;
    int skip;
    unsigned int flags;
    double last_x;
    double last_y;
} g_wt;

/* Not the current folder: only the driver's dll, or the app's own beside it. */
static int ng_windows_wintab_api(void) {
    if (!g_wt_api.looked_up) {
        g_wt_api.looked_up = 1;
        HMODULE dll = LoadLibraryExW(L"wintab32.dll", NULL,
                                     LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
        if (dll) {
            g_wt_api.info = (WtInfoFunc)GetProcAddress(dll, "WTInfoW");
            g_wt_api.open = (WtOpenFunc)GetProcAddress(dll, "WTOpenW");
            g_wt_api.close = (WtCloseFunc)GetProcAddress(dll, "WTClose");
            g_wt_api.packets_get = (WtPacketsGetFunc)GetProcAddress(dll, "WTPacketsGet");
            g_wt_api.queue_size_set = (WtQueueSizeSetFunc)GetProcAddress(dll, "WTQueueSizeSet");
        }
    }
    return g_wt_api.info && g_wt_api.open && g_wt_api.close && g_wt_api.packets_get && g_wt_api.queue_size_set;
}

/* Reports the stroke as dropped, once. */
void ng_windows_wintab_yield(void) {
    if (!g_wt.contact || !g_wt.owner) {
        g_wt.contact = 0;
        return;
    }
    g_wt.contact = 0;
    ng_invoke_pen((void*)g_wt.owner, g_wt.last_x, g_wt.last_y, 0.0f, 0.0f, 0.0f, 0.0f, AUREA_PEN_CANCEL, g_wt.flags,
                  ng_windows_modifiers(), 0.0);
}

static void ng_windows_wintab_close(void) {
    ng_windows_wintab_yield();
    if (g_wt.context) g_wt_api.close(g_wt.context);
    g_wt.context = NULL;
    g_wt.owner = NULL;
    g_wt.skip = 0;
}

static void ng_windows_wintab_open(HWND root) {
    if (!ng_windows_wintab_api() || g_wt_api.info(0, 0, NULL) == 0) return;
    WtContext lc;
    ZeroMemory(&lc, sizeof(lc));
    if (g_wt_api.info(WTI_DEFSYSCTX, 0, &lc) == 0) return;
    lc.lcOptions |= CXO_MESSAGES;
    lc.lcMsgBase = WT_DEFBASE;
    lc.lcPktData = AUREA_WT_PACKET_DATA;
    lc.lcPktMode = 0;
    lc.lcMoveMask = AUREA_WT_PACKET_DATA;
    lc.lcBtnDnMask = 0xFFFFFFFF;
    lc.lcBtnUpMask = 0xFFFFFFFF;
    /* All the tablet's resolution, top to bottom; mapped to the screen here, keeping fractions. */
    lc.lcOutOrgX = 0;
    lc.lcOutOrgY = 0;
    lc.lcOutExtX = lc.lcInExtX;
    lc.lcOutExtY = -lc.lcInExtY;

    HCTX context = g_wt_api.open(root, &lc, TRUE);
    if (!context) return;
    for (int size = AUREA_WT_QUEUE; size >= AUREA_WT_QUEUE_LEAST; size /= 2) {
        if (g_wt_api.queue_size_set(context, size)) break;
    }
    if (g_wt_api.info(WTI_DEVICES, DVC_NPRESSURE, &g_wt.pressure) == 0) ZeroMemory(&g_wt.pressure, sizeof(g_wt.pressure));
    if (g_wt_api.info(WTI_DEVICES, DVC_ORIENTATION, g_wt.orientation) == 0) {
        ZeroMemory(g_wt.orientation, sizeof(g_wt.orientation));
    }
    g_wt.context = context;
    g_wt.owner = root;
    g_wt.shape = lc;
}

/* Only the active window has a context, so one pen goes to one window. */
void ng_windows_wintab_sync(void) {
    HWND want = ng_windows_pen_mode() == AUREA_PEN_MODE_SYSTEM ? NULL : g_wt.active_root;
    if (g_wt.owner == want) return;
    ng_windows_wintab_close();
    if (want) ng_windows_wintab_open(want);
}

void ng_windows_wintab_activate(HWND root, int active) {
    if (active) {
        g_wt.active_root = root;
    } else if (g_wt.active_root == root) {
        g_wt.active_root = NULL;
    }
    ng_windows_wintab_sync();
}

void ng_windows_wintab_destroyed(HWND root) {
    ng_windows_wintab_activate(root, 0);
}

static double ng_windows_wintab_fraction(LONG value, LONG extent) {
    long long span = extent < 0 ? -(long long)extent : (long long)extent;
    return span > 0 ? (double)value / (double)span : 0.0;
}

static float ng_windows_wintab_pressure(const WtPacket* packet) {
    const WtAxis* axis = &g_wt.pressure;
    if (axis->axMax <= axis->axMin) return (packet->buttons & 1u) ? 1.0f : 0.0f;
    float value = (float)((LONG)packet->pressure - axis->axMin) / (float)(axis->axMax - axis->axMin);
    return value < 0.0f ? 0.0f : value > 1.0f ? 1.0f : value;
}

/* Azimuth is clockwise from up and altitude up from the tablet; a pen lying towards the right or
   the user tilts positive, as with Windows Ink. */
static void ng_windows_wintab_tilt(const WtPacket* packet, float* tilt_x, float* tilt_y) {
    *tilt_x = 0.0f;
    *tilt_y = 0.0f;
    LONG full_turn = g_wt.orientation[0].axMax;
    LONG upright = g_wt.orientation[1].axMax;
    if (full_turn <= 0 || upright <= 0) return;
    double altitude = fabs((double)packet->altitude) / (double)upright * (AUREA_WT_PI / 2.0);
    double azimuth = (double)packet->azimuth / (double)full_turn * (2.0 * AUREA_WT_PI);
    double x = cos(altitude) * sin(azimuth);
    double y = -cos(altitude) * cos(azimuth);
    double z = sin(altitude);
    *tilt_x = (float)(atan2(x, z) * 180.0 / AUREA_WT_PI);
    *tilt_y = (float)(atan2(y, z) * 180.0 / AUREA_WT_PI);
}

static float ng_windows_wintab_twist(const WtPacket* packet) {
    LONG full_turn = g_wt.orientation[2].axMax;
    return full_turn > 0 ? (float)packet->twist * 360.0f / (float)full_turn : 0.0f;
}

static double ng_windows_wintab_age(const WtPacket* packet) {
    DWORD ms = GetTickCount() - packet->time;
    return ms < 1000 ? (double)ms / 1000.0 : 0.0;
}

static int ng_windows_wintab_inside(HWND root, double sx, double sy) {
    POINT at = {(LONG)sx, (LONG)sy};
    HWND under = WindowFromPoint(at);
    return under && GetAncestor(under, GA_ROOT) == root;
}

static void ng_windows_wintab_packet(HWND root, const WtPacket* packet, POINT origin, double logical) {
    float pressure = ng_windows_wintab_pressure(packet);
    int in_range = !(packet->status & TPS_PROXIMITY);
    int contact = in_range && ((packet->buttons & 1u) || pressure > 0.0f);
    const WtContext* lc = &g_wt.shape;
    double sx = lc->lcSysOrgX + ng_windows_wintab_fraction(packet->x, lc->lcOutExtX) * lc->lcSysExtX;
    double sy = lc->lcSysOrgY + ng_windows_wintab_fraction(packet->y, lc->lcOutExtY) * lc->lcSysExtY;

    /* A stroke started outside belongs to whatever is there, and one started here stays here. */
    if (g_wt.skip) {
        if (contact) return;
        g_wt.skip = 0;
    }
    if (!g_wt.contact && !ng_windows_wintab_inside(root, sx, sy)) {
        g_wt.skip = contact;
        return;
    }

    unsigned int phase = contact ? (g_wt.contact ? AUREA_PEN_MOVE : AUREA_PEN_DOWN)
                                 : (g_wt.contact ? AUREA_PEN_UP : AUREA_PEN_HOVER);
    float tilt_x, tilt_y;
    ng_windows_wintab_tilt(packet, &tilt_x, &tilt_y);
    unsigned int flags = AUREA_PEN_WINTAB;
    if (packet->status & TPS_INVERT) flags |= AUREA_PEN_ERASER;
    if (packet->buttons & ~1u) flags |= AUREA_PEN_BARREL;

    g_wt.contact = contact;
    g_wt.flags = flags;
    g_wt.last_x = (sx - origin.x) / logical;
    g_wt.last_y = (sy - origin.y) / logical;
    ng_invoke_pen((void*)root, g_wt.last_x, g_wt.last_y, pressure, tilt_x, tilt_y, ng_windows_wintab_twist(packet),
                  phase, flags, ng_windows_modifiers(), ng_windows_wintab_age(packet));
}

static void ng_windows_wintab_drain(HWND root) {
    WtPacket packets[AUREA_WT_QUEUE];
    int count = g_wt_api.packets_get(g_wt.context, AUREA_WT_QUEUE, packets);
    if (count <= 0 || ng_windows_pen_ink_live()) return;
    POINT origin = {0, 0};
    ClientToScreen(root, &origin);
    float scale = ng_windows_get_scale_factor((NGHandle)root);
    double logical = scale > 1.0f ? (double)scale : 1.0;
    for (int i = 0; i < count; i++) {
        ng_windows_wintab_packet(root, &packets[i], origin, logical);
    }
}

void ng_windows_wintab_message(HWND root, UINT msg, LPARAM lParam) {
    if (root != g_wt.owner || !g_wt.context) return;
    if (msg == WT_PACKET) {
        ng_windows_wintab_drain(root);
    } else if (msg == WT_PROXIMITY && LOWORD(lParam) == 0 && g_wt.contact) {
        /* Out of range with no packet saying the tip lifted. */
        g_wt.contact = 0;
        ng_invoke_pen((void*)root, g_wt.last_x, g_wt.last_y, 0.0f, 0.0f, 0.0f, 0.0f, AUREA_PEN_UP, g_wt.flags,
                      ng_windows_modifiers(), 0.0);
    }
}
