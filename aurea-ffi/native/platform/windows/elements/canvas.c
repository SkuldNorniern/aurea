#include "common.h"
#include "../window_state.h"
#include "../elements.h"
#include "../window.h"
#include "../pen.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <windowsx.h>
#include <tpcshrd.h>
#include <stdlib.h>

/* Registered wide so typed text stays UTF-16: an ANSI window gets WM_CHAR as code page bytes. */
static const wchar_t* CANVAS_CLASS_NAME = L"AureaCanvas";

typedef struct {
    const unsigned char* buffer;
    unsigned int width;
    unsigned int height;
    int gpu_owned;
} CanvasState;

static CanvasState* canvas_state(HWND hwnd) {
    return (CanvasState*)GetWindowLongPtrW(hwnd, GWLP_USERDATA);
}

static HWND canvas_root(HWND hwnd) {
    return ng_windows_root_window(GetParent(hwnd));
}

/* Draws the app's buffer under `dirty`, stretched when the client area differs from it. */
static void paint_buffer(HDC hdc, HWND hwnd, const CanvasState* s, RECT dirty) {
    RECT client;
    GetClientRect(hwnd, &client);
    int client_w = client.right - client.left;
    int client_h = client.bottom - client.top;
    if (client_w <= 0 || client_h <= 0) return;

    BITMAPINFO bmi;
    ZeroMemory(&bmi, sizeof(bmi));
    bmi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bmi.bmiHeader.biWidth = (LONG)s->width;
    bmi.bmiHeader.biHeight = -(LONG)s->height;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    if (client_w == (int)s->width && client_h == (int)s->height) {
        int dirty_h = dirty.bottom - dirty.top;
        if (dirty_h > 0) {
            SetDIBitsToDevice(hdc, dirty.left, dirty.top, dirty.right - dirty.left, dirty_h,
                              dirty.left, (int)s->height - dirty.bottom, 0, s->height,
                              s->buffer, &bmi, DIB_RGB_COLORS);
        }
        return;
    }

    double sx = (double)s->width / client_w;
    double sy = (double)s->height / client_h;
    int src_x = (int)(dirty.left * sx);
    int src_y = (int)(dirty.top * sy);
    int src_w = (int)((dirty.right - dirty.left) * sx + 0.5);
    int src_h = (int)((dirty.bottom - dirty.top) * sy + 0.5);
    if (src_w > 0 && src_h > 0) {
        StretchDIBits(hdc, dirty.left, dirty.top, dirty.right - dirty.left, dirty.bottom - dirty.top,
                      src_x, src_y, src_w, src_h, s->buffer, &bmi, DIB_RGB_COLORS, SRCCOPY);
    }
}

static LRESULT CALLBACK CanvasProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    CanvasState* s = canvas_state(hwnd);

    switch (msg) {
    case WM_MOUSEACTIVATE: {
        HWND root = canvas_root(hwnd);
        if (root) {
            SetActiveWindow(root);
            SetFocus(root);
        }
        return MA_ACTIVATE;
    }

    case WM_SETFOCUS: {
        HWND root = canvas_root(hwnd);
        if (root) SetFocus(root);
        return 0;
    }

    case WM_KEYDOWN:
    case WM_KEYUP:
    case WM_CHAR:
    case WM_SYSCHAR:
    case WM_SYSKEYDOWN:
    case WM_SYSKEYUP: {
        HWND root = canvas_root(hwnd);
        if (root) return SendMessageW(root, msg, wParam, lParam);
        break;
    }

    case WM_SETCURSOR:
        SetCursor((HCURSOR)ng_windows_window_cursor((NGHandle)canvas_root(hwnd)));
        return TRUE;

    case WM_MOUSEMOVE:
    case WM_MOUSELEAVE:
    case WM_LBUTTONDOWN:
    case WM_LBUTTONDBLCLK:
    case WM_LBUTTONUP:
    case WM_RBUTTONDOWN:
    case WM_RBUTTONDBLCLK:
    case WM_RBUTTONUP:
    case WM_MBUTTONDOWN:
    case WM_MBUTTONDBLCLK:
    case WM_MBUTTONUP:
    case WM_XBUTTONDOWN:
    case WM_XBUTTONDBLCLK:
    case WM_XBUTTONUP:
    case WM_MOUSEWHEEL:
    case WM_MOUSEHWHEEL: {
        HWND root = canvas_root(hwnd);
        if (!root) break;
        switch (msg) {
        case WM_LBUTTONDOWN: case WM_LBUTTONDBLCLK:
        case WM_RBUTTONDOWN: case WM_RBUTTONDBLCLK:
        case WM_MBUTTONDOWN: case WM_MBUTTONDBLCLK:
        case WM_XBUTTONDOWN: case WM_XBUTTONDBLCLK:
            SetActiveWindow(root);
            SetFocus(root);
            break;
        }
        LPARAM out = lParam;
        if (msg != WM_MOUSEWHEEL && msg != WM_MOUSEHWHEEL && msg != WM_MOUSELEAVE) {
            POINT pt = { GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
            MapWindowPoints(hwnd, root, &pt, 1);
            out = MAKELPARAM(pt.x, pt.y);
        }
        return SendMessageW(root, msg, wParam, out);
    }

    case WM_POINTERDOWN:
    case WM_POINTERUPDATE:
    case WM_POINTERUP:
        ng_windows_pen_message(canvas_root(hwnd), msg, wParam);
        break;

    case WM_POINTERCAPTURECHANGED:
        ng_windows_pen_capture_lost(canvas_root(hwnd), wParam);
        break;

    case WM_TABLET_QUERYSYSTEMGESTURESTATUS: {
        LRESULT status;
        if (ng_windows_pen_query_gestures(&status)) return status;
        break;
    }

    case WM_SIZE:
        ng_invoke_canvas_resized((void*)hwnd);
        break;

    case WM_ERASEBKGND:
        return 1;

    case WM_PAINT: {
        PAINTSTRUCT ps;
        HDC hdc = BeginPaint(hwnd, &ps);
        if (s && s->gpu_owned) {
            /* The GPU draws here; leave the window alone. */
        } else if (s && s->buffer && s->width > 0 && s->height > 0) {
            paint_buffer(hdc, hwnd, s, ps.rcPaint);
        } else {
            FillRect(hdc, &ps.rcPaint, (HBRUSH)(COLOR_WINDOW + 1));
        }
        EndPaint(hwnd, &ps);
        return 0;
    }

    case WM_DESTROY:
        ng_invoke_canvas_destroyed((void*)hwnd);
        /* Clear first: after this the window has no state and later calls must find none. */
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        free(s);
        return 0;
    }

    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

static int canvas_class_ready(void) {
    static int registered = 0;
    if (registered) return 1;
    WNDCLASSEXW wc = {0};
    wc.cbSize = sizeof(WNDCLASSEXW);
    wc.style = CS_DBLCLKS;
    wc.lpfnWndProc = CanvasProc;
    wc.hInstance = GetModuleHandleW(NULL);
    wc.lpszClassName = CANVAS_CLASS_NAME;
    wc.hCursor = LoadCursor(NULL, IDC_ARROW);
    wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
    registered = RegisterClassExW(&wc) != 0;
    return registered;
}

NGHandle ng_windows_create_canvas(int width, int height) {
    if (!canvas_class_ready()) return NULL;

    HWND hwnd = CreateWindowExW(0, CANVAS_CLASS_NAME, NULL, WS_CHILD | WS_VISIBLE,
                                0, 0, width, height, ng_windows_detached_parent(), NULL,
                                GetModuleHandleW(NULL), NULL);
    if (!hwnd) return NULL;

    CanvasState* s = (CanvasState*)calloc(1, sizeof(CanvasState));
    if (!s) {
        DestroyWindow(hwnd);
        return NULL;
    }
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, (LONG_PTR)s);
    return (NGHandle)hwnd;
}

void ng_windows_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    InvalidateRect((HWND)canvas, NULL, FALSE);
}

void ng_windows_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    if (!canvas) return;
    if (!(width > 0.0f) || !(height > 0.0f)) return;
    if (!(x > -1e7f && y > -1e7f && x + width < 1e7f && y + height < 1e7f)) {
        InvalidateRect((HWND)canvas, NULL, FALSE);
        return;
    }
    RECT rect;
    rect.left = (LONG)x - ((float)(LONG)x > x);
    rect.top = (LONG)y - ((float)(LONG)y > y);
    rect.right = (LONG)(x + width) + ((float)(LONG)(x + width) < x + width);
    rect.bottom = (LONG)(y + height) + ((float)(LONG)(y + height) < y + height);
    InvalidateRect((HWND)canvas, &rect, FALSE);
}

void ng_windows_canvas_set_gpu_owned(NGHandle canvas, int gpu_owned) {
    if (!canvas) return;
    CanvasState* s = canvas_state((HWND)canvas);
    if (!s) return;
    s->gpu_owned = gpu_owned != 0;
    if (s->gpu_owned) {
        s->buffer = NULL;
        s->width = 0;
        s->height = 0;
    }
}

void ng_windows_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height) {
    (void)size;
    if (!canvas) return;
    CanvasState* s = canvas_state((HWND)canvas);
    if (!s) return;
    if (!buffer || width == 0 || height == 0) {
        s->buffer = NULL;
        s->width = 0;
        s->height = 0;
        return;
    }
    s->buffer = buffer;
    s->width = width;
    s->height = height;
}

void ng_windows_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    if (!canvas || !width || !height) return;
    RECT rect;
    if (GetClientRect((HWND)canvas, &rect)) {
        unsigned int dpi = ng_windows_dpi((HWND)canvas);
        if (dpi == 0) dpi = 96;
        unsigned int w = (unsigned int)(rect.right - rect.left);
        unsigned int h = (unsigned int)(rect.bottom - rect.top);
        *width = (w * 96 + dpi - 1) / dpi;
        *height = (h * 96 + dpi - 1) / dpi;
    }
}

void ng_windows_canvas_get_pixel_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    if (!canvas || !width || !height) return;
    RECT rect;
    if (GetClientRect((HWND)canvas, &rect)) {
        *width = (unsigned int)(rect.right - rect.left);
        *height = (unsigned int)(rect.bottom - rect.top);
    }
}

NGHandle ng_windows_canvas_get_window(NGHandle canvas) {
    if (!canvas) return NULL;
    return (NGHandle)canvas_root((HWND)canvas);
}

NGHandle ng_windows_canvas_get_native_handle(NGHandle canvas) {
    return canvas;
}
