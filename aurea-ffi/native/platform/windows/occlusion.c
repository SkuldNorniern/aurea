/* How much of a window shows: its on-screen rect less every visible window above it in z order. The
 * part off every screen does not show either. Minimized and cloaked (another virtual desktop) count
 * as covered; click-through overlays, mostly transparent layered windows, and this app's own menus,
 * tool windows and owned popups do not cover. */
#include "occlusion.h"
#include "utils.h"

#include <stdlib.h>

#define AUREA_DWMWA_EXTENDED_FRAME_BOUNDS 9
#define AUREA_DWMWA_CLOAKED 14

typedef HRESULT (WINAPI *DwmAttributeFunc)(HWND, DWORD, PVOID, DWORD);

static DwmAttributeFunc ng_windows_dwm_attribute(void) {
    static DwmAttributeFunc fn = NULL;
    static BOOL looked = FALSE;
    if (!looked) {
        fn = (DwmAttributeFunc)ng_windows_api("dwmapi.dll", "DwmGetWindowAttribute");
        looked = TRUE;
    }
    return fn;
}

static BOOL ng_windows_cloaked(HWND hwnd) {
    DwmAttributeFunc dwm = ng_windows_dwm_attribute();
    DWORD cloaked = 0;
    return dwm && SUCCEEDED(dwm(hwnd, AUREA_DWMWA_CLOAKED, &cloaked, sizeof cloaked)) && cloaked;
}

/* The rect a window shows on screen: without the invisible resize borders where DWM says. */
static BOOL ng_windows_shown_rect(HWND hwnd, RECT* rect) {
    DwmAttributeFunc dwm = ng_windows_dwm_attribute();
    if (dwm && SUCCEEDED(dwm(hwnd, AUREA_DWMWA_EXTENDED_FRAME_BOUNDS, rect, sizeof *rect))) {
        return TRUE;
    }
    return GetWindowRect(hwnd, rect);
}

/* Whether `above`, already known to overlap, hides what is under it. */
static BOOL ng_windows_covers(HWND above) {
    LONG_PTR ex = GetWindowLongPtrW(above, GWL_EXSTYLE);
    if (ex & WS_EX_TRANSPARENT) return FALSE;
    DWORD pid = 0;
    GetWindowThreadProcessId(above, &pid);
    if (pid == GetCurrentProcessId() &&
        ((ex & WS_EX_TOOLWINDOW) || GetWindow(above, GW_OWNER) || GetClassLongPtrW(above, GCW_ATOM) == 32768)) {
        return FALSE;
    }
    if (ng_windows_cloaked(above)) return FALSE;
    if (ex & WS_EX_LAYERED) {
        BYTE alpha = 255;
        DWORD flags = 0;
        if (GetLayeredWindowAttributes(above, NULL, &alpha, &flags) && (flags & LWA_ALPHA) &&
            alpha < 128) {
            return FALSE;
        }
    }
    return TRUE;
}

static LONGLONG ng_windows_region_area(HRGN region) {
    DWORD size = GetRegionData(region, 0, NULL);
    if (!size) return 0;
    RGNDATA* data = (RGNDATA*)malloc(size);
    if (!data) return 0;
    LONGLONG area = 0;
    if (GetRegionData(region, size, data)) {
        RECT* rects = (RECT*)data->Buffer;
        for (DWORD i = 0; i < data->rdh.nCount; i++) {
            area += (LONGLONG)(rects[i].right - rects[i].left) * (rects[i].bottom - rects[i].top);
        }
    }
    free(data);
    return area;
}

int ng_windows_occlusion_of(HWND hwnd) {
    if (!hwnd || !IsWindowVisible(hwnd) || IsIconic(hwnd) || ng_windows_cloaked(hwnd)) {
        return AUREA_OCCLUSION_COVERED;
    }
    RECT rect;
    if (!ng_windows_shown_rect(hwnd, &rect) || IsRectEmpty(&rect)) return AUREA_OCCLUSION_VISIBLE;

    /* Off every screen is out of sight too. */
    RECT screen = {
        GetSystemMetrics(SM_XVIRTUALSCREEN),
        GetSystemMetrics(SM_YVIRTUALSCREEN),
        GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN),
        GetSystemMetrics(SM_YVIRTUALSCREEN) + GetSystemMetrics(SM_CYVIRTUALSCREEN),
    };
    RECT on_screen;
    if (!IntersectRect(&on_screen, &rect, &screen)) return AUREA_OCCLUSION_COVERED;
    LONGLONG total = (LONGLONG)(rect.right - rect.left) * (rect.bottom - rect.top);

    HRGN shown = CreateRectRgnIndirect(&on_screen);
    if (!shown) return AUREA_OCCLUSION_VISIBLE;
    for (HWND above = GetWindow(hwnd, GW_HWNDPREV); above; above = GetWindow(above, GW_HWNDPREV)) {
        /* cheap checks first: most windows above are nowhere near this one */
        RECT rough, cover;
        if (!IsWindowVisible(above) || IsIconic(above)) continue;
        if (!GetWindowRect(above, &rough) || !IntersectRect(&rough, &rough, &on_screen)) continue;
        if (!ng_windows_covers(above) || !ng_windows_shown_rect(above, &cover)) continue;
        HRGN covered = CreateRectRgnIndirect(&cover);
        if (!covered) continue;
        int left = CombineRgn(shown, shown, covered, RGN_DIFF);
        DeleteObject(covered);
        if (left == NULLREGION) break;
    }
    LONGLONG visible = ng_windows_region_area(shown);
    DeleteObject(shown);
    return ng_windows_occlusion_level(visible, total);
}

int ng_windows_occlusion_level(LONGLONG visible, LONGLONG total) {
    if (total <= 0 || visible * 1000 >= total * 999) return AUREA_OCCLUSION_VISIBLE;
    if (visible * 1000 <= total) return AUREA_OCCLUSION_COVERED;
    return AUREA_OCCLUSION_PARTLY;
}
