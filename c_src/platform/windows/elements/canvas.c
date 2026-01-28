#include "common.h"
#include "../elements.h"
#include "../../../common/errors.h"
#include <windows.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    const unsigned char* buffer;
    unsigned int width;
    unsigned int height;
} CanvasData;

static const char* CANVAS_CLASS_NAME = "AureaCanvas";
static int canvas_class_registered = 0;

static LRESULT CALLBACK CanvasProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    CanvasData* data = (CanvasData*)GetWindowLongPtrA(hwnd, GWLP_USERDATA);
    
    switch (msg) {
        case WM_ERASEBKGND:
            return 1;
        case WM_PAINT: {
            PAINTSTRUCT ps;
            HDC hdc = BeginPaint(hwnd, &ps);
            
            if (data && data->buffer && data->width > 0 && data->height > 0) {
                RECT rect;
                GetClientRect(hwnd, &rect);
                int dest_width = rect.right - rect.left;
                int dest_height = rect.bottom - rect.top;
                if (dest_width <= 0 || dest_height <= 0) {
                    EndPaint(hwnd, &ps);
                    return 0;
                }

                BITMAPINFO bmi;
                ZeroMemory(&bmi, sizeof(bmi));
                bmi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
                bmi.bmiHeader.biWidth = (LONG)data->width;
                bmi.bmiHeader.biHeight = -(LONG)data->height;
                bmi.bmiHeader.biPlanes = 1;
                bmi.bmiHeader.biBitCount = 32;
                bmi.bmiHeader.biCompression = BI_RGB;
                
                StretchDIBits(
                    hdc,
                    0,
                    0,
                    dest_width,
                    dest_height,
                    0,
                    0,
                    (int)data->width,
                    (int)data->height,
                    data->buffer,
                    &bmi,
                    DIB_RGB_COLORS,
                    SRCCOPY
                );
            } else {
                RECT rect;
                GetClientRect(hwnd, &rect);
                FillRect(hdc, &rect, (HBRUSH)(COLOR_WINDOW + 1));
            }
            
            EndPaint(hwnd, &ps);
            return 0;
        }
        case WM_DESTROY:
            if (data) {
                free(data);
            }
            return 0;
    }
    
    return DefWindowProcA(hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_create_canvas(int width, int height) {
    HWND temp_parent = GetDesktopWindow();

    if (!canvas_class_registered) {
        WNDCLASSEXA wc = {0};
        wc.cbSize = sizeof(WNDCLASSEXA);
        wc.lpfnWndProc = CanvasProc;
        wc.hInstance = GetModuleHandleA(NULL);
        wc.lpszClassName = CANVAS_CLASS_NAME;
        wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
        RegisterClassExA(&wc);
        canvas_class_registered = 1;
    }
    
    HWND hwnd = CreateWindowExA(
        0,
        CANVAS_CLASS_NAME,
        NULL,
        WS_CHILD | WS_VISIBLE,
        0, 0, width, height,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (hwnd) {
        CanvasData* data = (CanvasData*)calloc(1, sizeof(CanvasData));
        if (!data) {
            DestroyWindow(hwnd);
            return NULL;
        }
        SetWindowLongPtrA(hwnd, GWLP_USERDATA, (LONG_PTR)data);
    }

    return (NGHandle)hwnd;
}

void ng_windows_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    InvalidateRect((HWND)canvas, NULL, FALSE);
}

void ng_windows_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    if (!canvas) return;
    RECT rect;
    rect.left = (LONG)x;
    rect.top = (LONG)y;
    rect.right = (LONG)(x + width);
    rect.bottom = (LONG)(y + height);
    InvalidateRect((HWND)canvas, &rect, FALSE);
}

void ng_windows_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height) {
    (void)size;
    if (!canvas || !buffer || width == 0 || height == 0) return;
    
    CanvasData* data = (CanvasData*)GetWindowLongPtrA((HWND)canvas, GWLP_USERDATA);
    if (!data) return;
    
    data->buffer = buffer;
    data->width = width;
    data->height = height;
    
    InvalidateRect((HWND)canvas, NULL, FALSE);
    UpdateWindow((HWND)canvas);
}

void ng_windows_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    if (!canvas || !width || !height) return;
    
    RECT rect;
    if (GetClientRect((HWND)canvas, &rect)) {
        *width = (unsigned int)(rect.right - rect.left);
        *height = (unsigned int)(rect.bottom - rect.top);
    }
}

NGHandle ng_windows_canvas_get_window(NGHandle canvas) {
    if (!canvas) return NULL;
    HWND hwnd = (HWND)canvas;
    HWND parent = GetParent(hwnd);
    while (parent && parent != GetDesktopWindow()) {
        char class_name[256];
        GetClassNameA(parent, class_name, sizeof(class_name));
        if (strcmp(class_name, "NativeGuiWindow") == 0) {
            return (NGHandle)parent;
        }
        parent = GetParent(parent);
    }
    return NULL;
}

NGHandle ng_windows_canvas_get_native_handle(NGHandle canvas) {
    if (!canvas) return NULL;
    return canvas;
}
