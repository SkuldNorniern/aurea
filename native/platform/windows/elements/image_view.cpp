#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>
#include <gdiplus.h>
#include <objidl.h>
#include <string.h>

#pragma comment(lib, "gdiplus.lib")
#pragma comment(lib, "ole32.lib")

// Scaling mode constants
#define IMAGE_SCALING_NONE 0
#define IMAGE_SCALING_ASPECT_FIT 1
#define IMAGE_SCALING_ASPECT_FILL 2
#define IMAGE_SCALING_FILL 3

typedef struct {
    HWND hwnd;
    HBITMAP hBitmap;
    int scaling_mode;
} ImageViewData;

static LRESULT CALLBACK ImageViewProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    ImageViewData* data = (ImageViewData*)GetWindowLongPtr(hwnd, GWLP_USERDATA);
    
    switch (msg) {
        case WM_PAINT: {
            PAINTSTRUCT ps;
            HDC hdc = BeginPaint(hwnd, &ps);
            
            if (data && data->hBitmap) {
                HDC memDC = CreateCompatibleDC(hdc);
                HBITMAP oldBitmap = (HBITMAP)SelectObject(memDC, data->hBitmap);
                
                BITMAP bm;
                GetObject(data->hBitmap, sizeof(BITMAP), &bm);
                
                RECT rect;
                GetClientRect(hwnd, &rect);
                
                if (data->scaling_mode == IMAGE_SCALING_NONE) {
                    BitBlt(hdc, 0, 0, bm.bmWidth, bm.bmHeight, memDC, 0, 0, SRCCOPY);
                } else {
                    StretchBlt(hdc, 0, 0, rect.right, rect.bottom, memDC, 0, 0, bm.bmWidth, bm.bmHeight, SRCCOPY);
                }
                
                SelectObject(memDC, oldBitmap);
                DeleteDC(memDC);
            } else {
                RECT rect;
                GetClientRect(hwnd, &rect);
                FillRect(hdc, &rect, (HBRUSH)(COLOR_WINDOW + 1));
            }
            
            EndPaint(hwnd, &ps);
            return 0;
        }
        case WM_DESTROY: {
            if (data) {
                if (data->hBitmap) {
                    DeleteObject(data->hBitmap);
                }
                free(data);
            }
            return 0;
        }
    }
    
    return DefWindowProc(hwnd, msg, wParam, lParam);
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
        wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
        RegisterClassExA(&wc);
        registered = 1;
    }
    
    HWND hwnd = CreateWindowExA(
        0,
        className,
        NULL,
        WS_CHILD | WS_VISIBLE,
        0, 0, 200, 150,
        GetDesktopWindow(),
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );
    
    if (!hwnd) return NULL;
    
    ImageViewData* data = (ImageViewData*)calloc(1, sizeof(ImageViewData));
    data->hwnd = hwnd;
    data->scaling_mode = IMAGE_SCALING_ASPECT_FIT;
    SetWindowLongPtr(hwnd, GWLP_USERDATA, (LONG_PTR)data);
    
    return (NGHandle)hwnd;
}

int ng_windows_image_view_load_from_path(NGHandle image_view, const char* path) {
    if (!image_view || !path) return NG_ERROR_INVALID_PARAMETER;
    
    HWND hwnd = (HWND)image_view;
    ImageViewData* data = (ImageViewData*)GetWindowLongPtr(hwnd, GWLP_USERDATA);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    
    Gdiplus::GdiplusStartupInput gdiplusStartupInput;
    ULONG_PTR gdiplusToken;
    Gdiplus::GdiplusStartup(&gdiplusToken, &gdiplusStartupInput, NULL);
    
    WCHAR wpath[MAX_PATH];
    MultiByteToWideChar(CP_UTF8, 0, path, -1, wpath, MAX_PATH);
    
    Gdiplus::Bitmap* bitmap = new Gdiplus::Bitmap(wpath);
    if (!bitmap || bitmap->GetLastStatus() != Gdiplus::Ok) {
        Gdiplus::GdiplusShutdown(gdiplusToken);
        if (bitmap) delete bitmap;
        return NG_ERROR_CREATION_FAILED;
    }
    
    HBITMAP hBitmap;
    bitmap->GetHBITMAP(Gdiplus::Color(255, 255, 255, 255), &hBitmap);
    delete bitmap;
    
    if (data->hBitmap) {
        DeleteObject(data->hBitmap);
    }
    data->hBitmap = hBitmap;
    
    Gdiplus::GdiplusShutdown(gdiplusToken);
    
    InvalidateRect(hwnd, NULL, TRUE);
    return NG_SUCCESS;
}

int ng_windows_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    if (!image_view || !data || size == 0) return NG_ERROR_INVALID_PARAMETER;
    
    HWND hwnd = (HWND)image_view;
    ImageViewData* viewData = (ImageViewData*)GetWindowLongPtr(hwnd, GWLP_USERDATA);
    if (!viewData) return NG_ERROR_INVALID_HANDLE;
    
    Gdiplus::GdiplusStartupInput gdiplusStartupInput;
    ULONG_PTR gdiplusToken;
    Gdiplus::GdiplusStartup(&gdiplusToken, &gdiplusStartupInput, NULL);
    
    IStream* stream = NULL;
    HGLOBAL hMem = GlobalAlloc(GMEM_MOVEABLE, size);
    if (!hMem) {
        Gdiplus::GdiplusShutdown(gdiplusToken);
        return NG_ERROR_CREATION_FAILED;
    }
    
    void* pMem = GlobalLock(hMem);
    memcpy(pMem, data, size);
    GlobalUnlock(hMem);
    
    CreateStreamOnHGlobal(hMem, FALSE, &stream);
    
    Gdiplus::Bitmap* bitmap = new Gdiplus::Bitmap(stream);
    if (!bitmap || bitmap->GetLastStatus() != Gdiplus::Ok) {
        stream->Release();
        GlobalFree(hMem);
        Gdiplus::GdiplusShutdown(gdiplusToken);
        if (bitmap) delete bitmap;
        return NG_ERROR_CREATION_FAILED;
    }
    
    HBITMAP hBitmap;
    bitmap->GetHBITMAP(Gdiplus::Color(255, 255, 255, 255), &hBitmap);
    delete bitmap;
    stream->Release();
    GlobalFree(hMem);
    
    if (viewData->hBitmap) {
        DeleteObject(viewData->hBitmap);
    }
    viewData->hBitmap = hBitmap;
    
    Gdiplus::GdiplusShutdown(gdiplusToken);
    
    InvalidateRect(hwnd, NULL, TRUE);
    return NG_SUCCESS;
}

void ng_windows_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    if (!image_view) return;
    
    HWND hwnd = (HWND)image_view;
    ImageViewData* data = (ImageViewData*)GetWindowLongPtr(hwnd, GWLP_USERDATA);
    if (data) {
        data->scaling_mode = scaling_mode;
        InvalidateRect(hwnd, NULL, TRUE);
    }
}

void ng_windows_image_view_invalidate(NGHandle image_view) {
    if (!image_view) return;
    InvalidateRect((HWND)image_view, NULL, TRUE);
}

#ifdef __cplusplus
}
#endif

