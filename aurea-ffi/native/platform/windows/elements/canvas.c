#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <windows.h>
#include <windowsx.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    const unsigned char* buffer;
    unsigned int width;
    unsigned int height;
    int gpu_owned;
} CanvasData;

static const char* CANVAS_CLASS_NAME = "AureaCanvas";
static int canvas_class_registered = 0;

/* Walk up the HWND tree to find the first ancestor with class "NativeGuiWindow". */
static HWND canvas_find_root_window(HWND start) {
    HWND p = GetParent(start);
    while (p && p != GetDesktopWindow()) {
        char cls[256];
        GetClassNameA(p, cls, sizeof(cls));
        if (strcmp(cls, "NativeGuiWindow") == 0) return p;
        p = GetParent(p);
    }
    return NULL;
}

static LRESULT CALLBACK CanvasProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    CanvasData* data = (CanvasData*)GetWindowLongPtrA(hwnd, GWLP_USERDATA);

    switch (msg) {
        /* Let clicks in a canvas activate the top-level window, then keep
           keyboard focus on that root window so WindowProc emits key events
           using the registered window handle. */
        case WM_MOUSEACTIVATE: {
            HWND root = canvas_find_root_window(hwnd);
            if (root) {
                SetActiveWindow(root);
                SetFocus(root);
            }
            return MA_ACTIVATE;
        }

        /* DefWindowProc for WM_LBUTTONDOWN calls SetFocus(hwnd), so the canvas
           can still receive WM_SETFOCUS.  Immediately give focus back to the
           NativeGuiWindow so all keyboard input is handled there. */
        case WM_SETFOCUS: {
            HWND root = canvas_find_root_window(hwnd);
            if (root) SetFocus(root);
            return 0;
        }

        /* Forward keyboard messages to the root NativeGuiWindow as a fallback
           (handles the case where focus arrives through another path). */
        case WM_KEYDOWN:
        case WM_KEYUP:
        case WM_CHAR:
        case WM_SYSCHAR:
        case WM_SYSKEYDOWN:
        case WM_SYSKEYUP: {
            HWND root = canvas_find_root_window(hwnd);
            if (root) return SendMessageA(root, msg, wParam, lParam);
            break;
        }

        case WM_SETCURSOR:
            SetCursor(LoadCursor(NULL, IDC_ARROW));
            return TRUE;

        case WM_MOUSEMOVE:
        case WM_MOUSELEAVE:
        case WM_LBUTTONDOWN:
        case WM_LBUTTONUP:
        case WM_RBUTTONDOWN:
        case WM_RBUTTONUP:
        case WM_MBUTTONDOWN:
        case WM_MBUTTONUP:
        case WM_XBUTTONDOWN:
        case WM_XBUTTONUP:
        case WM_MOUSEWHEEL:
        case WM_MOUSEHWHEEL: {
            HWND root = canvas_find_root_window(hwnd);
            if (root) {
                if (msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN || msg == WM_MBUTTONDOWN || msg == WM_XBUTTONDOWN) {
                    SetActiveWindow(root);
                    SetFocus(root);
                }
                LPARAM out_lParam = lParam;
                /* WM_MOUSEWHEEL/HWHEEL carry screen coordinates (unused by the
                   root handler) and WM_MOUSELEAVE carries none — only
                   position-bearing messages need translation from this
                   canvas's client space into the root window's client space. */
                if (msg != WM_MOUSEWHEEL && msg != WM_MOUSEHWHEEL && msg != WM_MOUSELEAVE) {
                    POINT pt = { GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
                    MapWindowPoints(hwnd, root, &pt, 1);
                    out_lParam = MAKELPARAM(pt.x, pt.y);
                }
                return SendMessageA(root, msg, wParam, out_lParam);
            }
            break;
        }

        case WM_ERASEBKGND:
            return 1;
        case WM_PAINT: {
            PAINTSTRUCT ps;
            HDC hdc = BeginPaint(hwnd, &ps);

            if (data && data->gpu_owned) {
                EndPaint(hwnd, &ps);
                return 0;
            }

            if (data && data->buffer && data->width > 0 && data->height > 0) {
                RECT client;
                GetClientRect(hwnd, &client);
                int client_w = client.right - client.left;
                int client_h = client.bottom - client.top;
                if (client_w <= 0 || client_h <= 0) {
                    EndPaint(hwnd, &ps);
                    return 0;
                }

                // Clip the blit to the dirty rect reported by BeginPaint.
                // When invalidate_rect was used this is only the damaged region;
                // after a full invalidate it covers the whole client area.
                RECT dirty = ps.rcPaint;

                BITMAPINFO bmi;
                ZeroMemory(&bmi, sizeof(bmi));
                bmi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
                bmi.bmiHeader.biWidth = (LONG)data->width;
                bmi.bmiHeader.biHeight = -(LONG)data->height;
                bmi.bmiHeader.biPlanes = 1;
                bmi.bmiHeader.biBitCount = 32;
                bmi.bmiHeader.biCompression = BI_RGB;

                if (client_w == (int)data->width && client_h == (int)data->height) {
                    // 1:1 — no scaling. Use SetDIBitsToDevice and copy only the
                    // dirty rows directly from the Rust framebuffer.
                    int dirty_h = dirty.bottom - dirty.top;
                    if (dirty_h > 0) {
                        SetDIBitsToDevice(
                            hdc,
                            dirty.left,
                            dirty.top,
                            dirty.right - dirty.left,
                            dirty_h,
                            dirty.left,
                            (int)data->height - dirty.bottom, // DIB is bottom-up, flip
                            0,
                            data->height,
                            data->buffer,
                            &bmi,
                            DIB_RGB_COLORS
                        );
                    }
                } else {
                    // Scaled: map dirty dest rect back to source coords and blit
                    // only that sub-region to avoid processing the full bitmap.
                    double sx = (double)data->width  / client_w;
                    double sy = (double)data->height / client_h;
                    int src_x = (int)(dirty.left   * sx);
                    int src_y = (int)(dirty.top    * sy);
                    int src_w = (int)((dirty.right  - dirty.left) * sx + 0.5);
                    int src_h = (int)((dirty.bottom - dirty.top)  * sy + 0.5);
                    if (src_w > 0 && src_h > 0) {
                        StretchDIBits(
                            hdc,
                            dirty.left, dirty.top,
                            dirty.right - dirty.left, dirty.bottom - dirty.top,
                            src_x, src_y, src_w, src_h,
                            data->buffer,
                            &bmi,
                            DIB_RGB_COLORS,
                            SRCCOPY
                        );
                    }
                }
            } else {
                FillRect(hdc, &ps.rcPaint, (HBRUSH)(COLOR_WINDOW + 1));
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
        wc.hCursor = LoadCursor(NULL, IDC_ARROW);
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

void ng_windows_canvas_set_gpu_owned(NGHandle canvas, int gpu_owned) {
    if (!canvas) return;
    CanvasData* data = (CanvasData*)GetWindowLongPtrA((HWND)canvas, GWLP_USERDATA);
    if (!data) return;
    data->gpu_owned = gpu_owned != 0;
    if (data->gpu_owned) {
        data->buffer = NULL;
        data->width = 0;
        data->height = 0;
    }
}

void ng_windows_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height) {
    (void)size;
    if (!canvas || !buffer || width == 0 || height == 0) return;
    
    CanvasData* data = (CanvasData*)GetWindowLongPtrA((HWND)canvas, GWLP_USERDATA);
    if (!data) return;
    
    data->buffer = buffer;
    data->width = width;
    data->height = height;
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
