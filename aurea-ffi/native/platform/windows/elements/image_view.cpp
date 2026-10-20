#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include "common/image_fit.h"
#include <windows.h>
#include <gdiplus.h>
#include <objidl.h>
#include <stdlib.h>
#include <string.h>

#pragma comment(lib, "gdiplus.lib")
#pragma comment(lib, "ole32.lib")

typedef struct {
    HWND hwnd;
    HBITMAP hBitmap;
    int scaling_mode;
} ImageViewData;

static RECT image_rect(int mode, RECT area, int w, int h) {
    NGImageRect fit = ng_image_fit(mode, area.right - area.left, area.bottom - area.top, w, h);
    RECT r;
    r.left = area.left + (int)(fit.x + 0.5);
    r.top = area.top + (int)(fit.y + 0.5);
    r.right = r.left + (int)(fit.width + 0.5);
    r.bottom = r.top + (int)(fit.height + 0.5);
    return r;
}

static void paint(HWND hwnd, ImageViewData* data, HDC hdc) {
    RECT client;
    GetClientRect(hwnd, &client);
    if (!data || !data->hBitmap) {
        FillRect(hdc, &client, GetSysColorBrush(COLOR_WINDOW));
        return;
    }
    BITMAP bm;
    GetObject(data->hBitmap, sizeof(BITMAP), &bm);
    RECT r = image_rect(data->scaling_mode, client, bm.bmWidth, bm.bmHeight);
    int saved = SaveDC(hdc);
    ExcludeClipRect(hdc, r.left, r.top, r.right, r.bottom);
    FillRect(hdc, &client, GetSysColorBrush(COLOR_WINDOW));
    RestoreDC(hdc, saved);

    saved = SaveDC(hdc);
    HDC memDC = CreateCompatibleDC(hdc);
    HBITMAP oldBitmap = (HBITMAP)SelectObject(memDC, data->hBitmap);
    IntersectClipRect(hdc, client.left, client.top, client.right, client.bottom);
    SetStretchBltMode(hdc, HALFTONE);
    SetBrushOrgEx(hdc, 0, 0, NULL);
    StretchBlt(hdc, r.left, r.top, r.right - r.left, r.bottom - r.top,
               memDC, 0, 0, bm.bmWidth, bm.bmHeight, SRCCOPY);
    SelectObject(memDC, oldBitmap);
    DeleteDC(memDC);
    RestoreDC(hdc, saved);
}

static LRESULT CALLBACK ImageViewProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    ImageViewData* data = (ImageViewData*)GetWindowLongPtr(hwnd, GWLP_USERDATA);

    switch (msg) {
        case WM_ERASEBKGND:
            return 1;
        case WM_PAINT: {
            PAINTSTRUCT ps;
            HDC hdc = BeginPaint(hwnd, &ps);
            paint(hwnd, data, hdc);
            EndPaint(hwnd, &ps);
            return 0;
        }
        case WM_PRINTCLIENT:
            paint(hwnd, data, (HDC)wParam);
            return 0;
        case WM_SIZE:
            InvalidateRect(hwnd, NULL, FALSE);
            break;
        case WM_NCDESTROY: {
            if (data) {
                if (data->hBitmap) {
                    DeleteObject(data->hBitmap);
                }
                free(data);
                SetWindowLongPtr(hwnd, GWLP_USERDATA, 0);
            }
            break;
        }
    }

    return DefWindowProc(hwnd, msg, wParam, lParam);
}

static HBITMAP decode(Gdiplus::Bitmap* bitmap) {
    HBITMAP result = NULL;
    if (bitmap && bitmap->GetLastStatus() == Gdiplus::Ok) {
        COLORREF bg = GetSysColor(COLOR_WINDOW);
        Gdiplus::Color under(255, GetRValue(bg), GetGValue(bg), GetBValue(bg));
        if (bitmap->GetHBITMAP(under, &result) != Gdiplus::Ok) result = NULL;
    }
    delete bitmap;
    return result;
}

static int set_bitmap(HWND hwnd, ImageViewData* data, HBITMAP bitmap) {
    if (!bitmap) return NG_ERROR_CREATION_FAILED;
    if (data->hBitmap) {
        DeleteObject(data->hBitmap);
    }
    data->hBitmap = bitmap;
    InvalidateRect(hwnd, NULL, FALSE);
    return NG_SUCCESS;
}

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_windows_create_image_view(void) {
    static const char* className = "AureaImageView";
    static int registered = 0;

    if (!registered) {
        WNDCLASSEXA wc = {0};
        wc.cbSize = sizeof(WNDCLASSEXA);
        wc.lpfnWndProc = ImageViewProc;
        wc.hInstance = GetModuleHandleA(NULL);
        wc.lpszClassName = className;
        wc.hCursor = LoadCursor(NULL, IDC_ARROW);
        RegisterClassExA(&wc);
        registered = 1;
    }

    ImageViewData* data = (ImageViewData*)calloc(1, sizeof(ImageViewData));
    if (!data) return NULL;

    HWND hwnd = CreateWindowExA(
        0,
        className,
        NULL,
        WS_CHILD | WS_VISIBLE,
        0, 0, 200, 150,
        ng_windows_detached_parent(),
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (!hwnd) {
        free(data);
        return NULL;
    }

    data->hwnd = hwnd;
    data->scaling_mode = NG_IMAGE_SCALING_ASPECT_FIT;
    SetWindowLongPtr(hwnd, GWLP_USERDATA, (LONG_PTR)data);

    return (NGHandle)hwnd;
}

int ng_windows_image_view_load_from_path(NGHandle image_view, const char* path) {
    if (!image_view || !path) return NG_ERROR_INVALID_PARAMETER;

    HWND hwnd = (HWND)image_view;
    ImageViewData* data = (ImageViewData*)GetWindowLongPtr(hwnd, GWLP_USERDATA);
    if (!data) return NG_ERROR_INVALID_HANDLE;

    wchar_t* wpath = ng_windows_utf8_to_wide(path);
    if (!wpath) return NG_ERROR_INVALID_PARAMETER;

    Gdiplus::GdiplusStartupInput gdiplusStartupInput;
    ULONG_PTR gdiplusToken;
    Gdiplus::GdiplusStartup(&gdiplusToken, &gdiplusStartupInput, NULL);
    HBITMAP bitmap = decode(new Gdiplus::Bitmap(wpath));
    Gdiplus::GdiplusShutdown(gdiplusToken);
    free(wpath);

    return set_bitmap(hwnd, data, bitmap);
}

int ng_windows_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    if (!image_view || !data || size == 0) return NG_ERROR_INVALID_PARAMETER;

    HWND hwnd = (HWND)image_view;
    ImageViewData* viewData = (ImageViewData*)GetWindowLongPtr(hwnd, GWLP_USERDATA);
    if (!viewData) return NG_ERROR_INVALID_HANDLE;

    HGLOBAL hMem = GlobalAlloc(GMEM_MOVEABLE, size);
    if (!hMem) return NG_ERROR_CREATION_FAILED;
    void* pMem = GlobalLock(hMem);
    if (!pMem) {
        GlobalFree(hMem);
        return NG_ERROR_CREATION_FAILED;
    }
    memcpy(pMem, data, size);
    GlobalUnlock(hMem);

    IStream* stream = NULL;
    if (FAILED(CreateStreamOnHGlobal(hMem, FALSE, &stream)) || !stream) {
        GlobalFree(hMem);
        return NG_ERROR_CREATION_FAILED;
    }

    Gdiplus::GdiplusStartupInput gdiplusStartupInput;
    ULONG_PTR gdiplusToken;
    Gdiplus::GdiplusStartup(&gdiplusToken, &gdiplusStartupInput, NULL);
    HBITMAP bitmap = decode(new Gdiplus::Bitmap(stream));
    Gdiplus::GdiplusShutdown(gdiplusToken);
    stream->Release();
    GlobalFree(hMem);

    return set_bitmap(hwnd, viewData, bitmap);
}

void ng_windows_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    if (!image_view) return;

    HWND hwnd = (HWND)image_view;
    ImageViewData* data = (ImageViewData*)GetWindowLongPtr(hwnd, GWLP_USERDATA);
    if (data) {
        data->scaling_mode = scaling_mode;
        InvalidateRect(hwnd, NULL, FALSE);
    }
}

void ng_windows_image_view_invalidate(NGHandle image_view) {
    if (!image_view) return;
    InvalidateRect((HWND)image_view, NULL, FALSE);
}

#ifdef __cplusplus
}
#endif
