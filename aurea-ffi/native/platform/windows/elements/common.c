#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <richedit.h>
#include <stdlib.h>
#include <string.h>

#define TEXT_ID_PROP "AureaTextId"
#define TEXT_KIND_PROP "AureaTextKind"

int get_box_orientation(HWND box) {
    return (INT_PTR)GetPropA(box, BOX_ORIENTATION_PROP) != BOX_HORIZONTAL;
}

static WNDPROC g_parking_proc = NULL;

static LRESULT CALLBACK parking_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    HWND from = NULL;
    if (msg == WM_COMMAND) from = (HWND)lParam;
    else if (msg == WM_NOTIFY && lParam) from = ((NMHDR*)lParam)->hwndFrom;
    else if (msg >= WM_CTLCOLORMSGBOX && msg <= WM_CTLCOLORSTATIC) from = (HWND)lParam;
    if (from) {
        HWND parent = GetParent(from);
        if (parent && parent != hwnd) return SendMessageW(parent, msg, wParam, lParam);
    }
    return CallWindowProcA(g_parking_proc, hwnd, msg, wParam, lParam);
}

HWND ng_windows_detached_parent(void) {
    static HWND parking = NULL;
    if (!parking || !IsWindow(parking)) {
        parking = CreateWindowExA(WS_EX_TOOLWINDOW, "STATIC", NULL, WS_POPUP,
                                  0, 0, 0, 0, NULL, NULL, GetModuleHandleA(NULL), NULL);
        if (parking) {
            g_parking_proc = (WNDPROC)SetWindowLongPtrA(parking, GWLP_WNDPROC, (LONG_PTR)parking_proc);
        }
    }
    return parking ? parking : GetDesktopWindow();
}

UINT ng_windows_dpi(HWND hwnd) {
    typedef UINT (WINAPI *GetDpiForWindowFunc)(HWND);
    static GetDpiForWindowFunc get_dpi = NULL;
    static BOOL looked_up = FALSE;
    if (!looked_up) {
        HMODULE user32 = GetModuleHandleA("user32.dll");
        if (user32) get_dpi = (GetDpiForWindowFunc)GetProcAddress(user32, "GetDpiForWindow");
        looked_up = TRUE;
    }
    UINT dpi = (get_dpi && hwnd) ? get_dpi(hwnd) : 0;
    if (dpi == 0) {
        HDC screen = GetDC(NULL);
        dpi = screen ? (UINT)GetDeviceCaps(screen, LOGPIXELSX) : 96;
        if (screen) ReleaseDC(NULL, screen);
    }
    return dpi ? dpi : 96;
}

int ng_windows_px(HWND hwnd, int logical) {
    return MulDiv(logical, (int)ng_windows_dpi(hwnd), 96);
}

#define AUREA_FONT_SLOTS 8
static struct { UINT dpi; HFONT font; } g_ui_fonts[AUREA_FONT_SLOTS];

static HFONT ng_windows_make_ui_font(UINT dpi) {
    typedef BOOL (WINAPI *ForDpiFunc)(UINT, UINT, PVOID, UINT, UINT);
    HMODULE user32 = GetModuleHandleA("user32.dll");
    ForDpiFunc for_dpi = user32
        ? (ForDpiFunc)GetProcAddress(user32, "SystemParametersInfoForDpi")
        : NULL;
    NONCLIENTMETRICSW metrics;
    metrics.cbSize = sizeof(metrics);
    BOOL got = for_dpi
        ? for_dpi(SPI_GETNONCLIENTMETRICS, sizeof(metrics), &metrics, 0, dpi)
        : SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, sizeof(metrics), &metrics, 0);
    if (!got) return (HFONT)GetStockObject(DEFAULT_GUI_FONT);
    if (!for_dpi) {
        HDC screen = GetDC(NULL);
        int system = screen ? GetDeviceCaps(screen, LOGPIXELSY) : 96;
        if (screen) ReleaseDC(NULL, screen);
        metrics.lfMessageFont.lfHeight = MulDiv(metrics.lfMessageFont.lfHeight, (int)dpi, system);
    }
    HFONT font = CreateFontIndirectW(&metrics.lfMessageFont);
    return font ? font : (HFONT)GetStockObject(DEFAULT_GUI_FONT);
}

HFONT ng_windows_ui_font(HWND hwnd) {
    UINT dpi = ng_windows_dpi(hwnd);
    for (int i = 0; i < AUREA_FONT_SLOTS; i++) {
        if (g_ui_fonts[i].font && g_ui_fonts[i].dpi == dpi) return g_ui_fonts[i].font;
    }
    for (int i = 0; i < AUREA_FONT_SLOTS; i++) {
        if (!g_ui_fonts[i].font) {
            g_ui_fonts[i].dpi = dpi;
            g_ui_fonts[i].font = ng_windows_make_ui_font(dpi);
            return g_ui_fonts[i].font;
        }
    }
    g_ui_fonts[AUREA_FONT_SLOTS - 1].dpi = dpi;
    g_ui_fonts[AUREA_FONT_SLOTS - 1].font = ng_windows_make_ui_font(dpi);
    return g_ui_fonts[AUREA_FONT_SLOTS - 1].font;
}

static BOOL ng_windows_is_ui_font(HFONT font) {
    for (int i = 0; i < AUREA_FONT_SLOTS; i++) {
        if (g_ui_fonts[i].font && g_ui_fonts[i].font == font) return TRUE;
    }
    return FALSE;
}

static BOOL CALLBACK ng_windows_refont_child(HWND child, LPARAM font) {
    HFONT current = (HFONT)SendMessageA(child, WM_GETFONT, 0, 0);
    if (current && ng_windows_is_ui_font(current) && current != (HFONT)font) {
        SendMessageA(child, WM_SETFONT, (WPARAM)font, TRUE);
    }
    return TRUE;
}

void ng_windows_refresh_dpi(HWND root) {
    if (!root) return;
    EnumChildWindows(root, ng_windows_refont_child, (LPARAM)ng_windows_ui_font(root));
    HWND content = (HWND)GetPropA(root, "AureaContentHWND");
    if (content && IsWindow(content) && is_box(content)) {
        layout_box_children(content);
    }
}

int is_box(HWND hwnd) {
    return GetPropA(hwnd, BOX_ORIENTATION_PROP) != NULL;
}

HWND ng_windows_create_control(DWORD ex_style, const wchar_t* cls, const char* text,
                               DWORD style, int width, int height, HWND parent, UINT id) {
    wchar_t* wide = ng_windows_utf8_to_wide(text ? text : "");
    if (!wide) return NULL;
    HWND hwnd = CreateWindowExW(ex_style, cls, wide, style, 0, 0, width, height, parent,
                                (HMENU)(UINT_PTR)id, GetModuleHandleW(NULL), NULL);
    free(wide);
    return hwnd;
}

void ng_windows_watch_text(HWND edit, unsigned int id, int kind) {
    SetPropA(edit, TEXT_ID_PROP, (HANDLE)(UINT_PTR)id);
    SetPropA(edit, TEXT_KIND_PROP, (HANDLE)(INT_PTR)kind);
    LRESULT mask = SendMessageW(edit, EM_GETEVENTMASK, 0, 0);
    SendMessageW(edit, EM_SETEVENTMASK, 0, mask | ENM_CHANGE);
}

void ng_windows_text_changed(HWND edit) {
    unsigned int id = (unsigned int)(UINT_PTR)GetPropA(edit, TEXT_ID_PROP);
    if (!id) return;
    char* text = ng_windows_window_text(edit);
    if (!text) return;
    if ((INT_PTR)GetPropA(edit, TEXT_KIND_PROP) == TEXT_CHANGE_EDITOR) {
        ng_invoke_text_callback(id, text);
    } else {
        ng_invoke_textview_callback(id, text);
    }
    free(text);
}

void ng_windows_relayout(HWND hwnd) {
    HWND top = NULL;
    for (HWND at = GetParent(hwnd); at && is_box(at); at = GetParent(at)) top = at;
    if (top) layout_box_children(top);
}

void calculate_text_size(HDC hdc, const char* text, int* width, int* height) {
    LOG_TRACE("calculate_text_size: called with text='%s'", text ? text : "(null)");
    
    if (!hdc || !text || !width || !height) {
        LOG_WARN("calculate_text_size: Invalid parameters");
        return;
    }
    
    SIZE text_size;
    wchar_t* wide = ng_windows_utf8_to_wide(text);

    if (wide && GetTextExtentPoint32W(hdc, wide, (int)wcslen(wide), &text_size)) {
        *width = text_size.cx;
        *height = text_size.cy;
    } else {
        *width = 100;
        *height = 20;
    }
    free(wide);
}

typedef struct {
    HWND hwnd;
    INT_PTR index;
    float weight;
    char class_name[64];
    int w;
    int h;
} BoxChild;

#define MAX_BOX_CHILDREN 256

static int box_padding(HWND box) {
    HWND parent = GetParent(box);
    char parent_class[64];
    if (parent && GetClassNameA(parent, parent_class, sizeof(parent_class)) &&
        _stricmp(parent_class, "NativeGuiWindow") == 0) {
        return ng_windows_px(box, PADDING);
    }
    return 0;
}

static int box_children(HWND box, BoxChild* out) {
    int n = 0;
    for (HWND child = GetWindow(box, GW_CHILD); child && n < MAX_BOX_CHILDREN;
         child = GetWindow(child, GW_HWNDNEXT)) {
        if (!(GetWindowLongPtrA(child, GWL_STYLE) & WS_VISIBLE)) continue;
        BoxChild c = {0};
        c.hwnd = child;
        c.index = (INT_PTR)GetPropA(child, BOX_INDEX_PROP);
        INT_PTR weight = (INT_PTR)GetPropA(child, BOX_WEIGHT_PROP);
        c.weight = weight > 0 ? (float)(weight - 1) / 1000.0f : 0.0f;
        GetClassNameA(child, c.class_name, sizeof(c.class_name));
        int at = n++;
        while (at > 0 && out[at - 1].index > c.index) {
            out[at] = out[at - 1];
            at--;
        }
        out[at] = c;
    }
    return n;
}

static HFONT control_font(HWND hwnd) {
    HFONT font = (HFONT)SendMessageW(hwnd, WM_GETFONT, 0, 0);
    return font ? font : (HFONT)GetStockObject(SYSTEM_FONT);
}

static SIZE text_extent(HWND hwnd, int wrap) {
    SIZE size = {0, 0};
    int len = GetWindowTextLengthW(hwnd);
    wchar_t* text = (wchar_t*)malloc(((size_t)len + 1) * sizeof(wchar_t));
    if (!text) return size;
    len = len > 0 ? GetWindowTextW(hwnd, text, len + 1) : 0;
    HDC hdc = GetDC(hwnd);
    if (!hdc) {
        free(text);
        return size;
    }
    HFONT old = (HFONT)SelectObject(hdc, control_font(hwnd));
    if (len > 0) {
        RECT r = {0, 0, wrap > 0 ? wrap : 0, 0};
        UINT flags = DT_LEFT | DT_CALCRECT | (wrap > 0 ? DT_WORDBREAK : DT_SINGLELINE);
        DrawTextW(hdc, text, len, &r, flags);
        size.cx = r.right;
        size.cy = r.bottom;
    } else {
        TEXTMETRICW tm;
        GetTextMetricsW(hdc, &tm);
        size.cy = tm.tmHeight;
    }
    SelectObject(hdc, old);
    ReleaseDC(hwnd, hdc);
    free(text);
    return size;
}

static void measure_box(HWND box, int in_vertical, int cross, int* out_w, int* out_h);

static void measure_child(BoxChild* c, int vertical, int cross) {
    RECT r;
    GetWindowRect(c->hwnd, &r);
    int w = r.right - r.left;
    int h = r.bottom - r.top;
    INT_PTR natural = (INT_PTR)GetPropA(c->hwnd, BOX_NATURAL_PROP);
    if (natural) {
        w = (int)((natural >> 16) & 0xFFFF);
        h = (int)(natural & 0xFFFF);
    }
    const char* cls = c->class_name;

    if (is_box(c->hwnd)) {
        measure_box(c->hwnd, vertical, cross, &w, &h);
    } else if (ng_windows_tab_bar_measure(c->hwnd, &w, &h)) {
    } else if (_stricmp(cls, "STATIC") == 0 && !GetPropA(c->hwnd, BOX_LABEL_PROP)) {
        if (vertical && cross > 0) w = cross;
    } else if (_stricmp(cls, "STATIC") == 0) {
        LONG_PTR style = GetWindowLongPtrA(c->hwnd, GWL_STYLE);
        if ((style & SS_TYPEMASK) == SS_LEFTNOWORDWRAP) {
            SetWindowLongPtrA(c->hwnd, GWL_STYLE, (style & ~SS_TYPEMASK) | SS_LEFT);
        }
        int empty = GetWindowTextLengthW(c->hwnd) == 0;
        if (vertical) {
            SIZE text = text_extent(c->hwnd, cross > 0 ? cross : 0);
            w = cross > 0 ? cross : text.cx;
            h = empty ? 0 : text.cy + ng_windows_px(c->hwnd, LABEL_PADDING * 2);
        } else {
            SIZE text = text_extent(c->hwnd, 0);
            w = empty ? 0 : text.cx + 2;
            h = empty ? 0 : text.cy + ng_windows_px(c->hwnd, LABEL_PADDING * 2);
        }
    } else if (_stricmp(cls, "BUTTON") == 0) {
        LONG_PTR type = GetWindowLongPtrA(c->hwnd, GWL_STYLE) & BS_TYPEMASK;
        SIZE text = text_extent(c->hwnd, 0);
        if (type == BS_AUTOCHECKBOX || type == BS_CHECKBOX || type == BS_AUTORADIOBUTTON ||
            type == BS_RADIOBUTTON || type == BS_AUTO3STATE || type == BS_3STATE) {
            int check = ng_windows_px(c->hwnd, 24);
            int min_h = ng_windows_px(c->hwnd, 20);
            w = vertical && cross > 0 ? cross : text.cx + check;
            h = text.cy + ng_windows_px(c->hwnd, 8);
            if (h < min_h) h = min_h;
        } else {
            int natural = text.cx + ng_windows_px(c->hwnd, 32);
            int min_w = ng_windows_px(c->hwnd, BUTTON_MIN_WIDTH);
            int min_h = ng_windows_px(c->hwnd, BUTTON_MIN_HEIGHT);
            w = natural > min_w ? natural : min_w;
            if (h < min_h) h = min_h;
        }
    } else if (vertical && cross > 0 &&
               (c->weight > 0.0f || _stricmp(cls, "RichEdit20W") == 0 || _stricmp(cls, "EDIT") == 0 ||
                _stricmp(cls, "AureaCanvas") == 0 || _stricmp(cls, "msctls_progress32") == 0 ||
                _stricmp(cls, "COMBOBOX") == 0)) {
        w = cross;
    }
    c->w = w;
    c->h = h;
}

static void measure_box(HWND box, int in_vertical, int cross, int* out_w, int* out_h) {
    BoxChild children[MAX_BOX_CHILDREN];
    int n = box_children(box, children);
    int vertical = get_box_orientation(box);
    int pad = box_padding(box);
    int spacing = ng_windows_px(box, SPACING);
    int width = in_vertical && cross > 0 ? cross : -1;
    int main = 0, across = 0;
    for (int i = 0; i < n; i++) {
        measure_child(&children[i], vertical,
                      vertical && width > 0 ? width - 2 * pad : -1);
        int m = vertical ? children[i].h : children[i].w;
        int a = vertical ? children[i].w : children[i].h;
        main += m + (i > 0 ? spacing : 0);
        if (a > across) across = a;
    }
    if (vertical) {
        *out_w = width > 0 ? width : across + 2 * pad;
        *out_h = main + 2 * pad;
    } else {
        *out_w = width > 0 ? width : main + 2 * pad;
        *out_h = across + 2 * pad;
    }
}

void layout_box_children(HWND box) {
    if (!box || !IsWindow(box)) {
        LOG_WARN("layout_box_children: Invalid box");
        return;
    }

    int vertical = get_box_orientation(box);
    RECT box_rect;
    GetClientRect(box, &box_rect);
    int box_width = box_rect.right - box_rect.left;
    int box_height = box_rect.bottom - box_rect.top;

    HWND parent = GetParent(box);
    if (parent) {
        char parent_class[64];
        GetClassNameA(parent, parent_class, sizeof(parent_class));
        if (_stricmp(parent_class, "NativeGuiWindow") == 0) {
            RECT parent_rect;
            GetClientRect(parent, &parent_rect);
            box_width = parent_rect.right - parent_rect.left;
            if (box_rect.right - box_rect.left != box_width) {
                SetWindowPos(box, NULL, 0, 0, box_width, box_height,
                             SWP_NOMOVE | SWP_NOZORDER);
            }
        }
    }

    BoxChild children[MAX_BOX_CHILDREN];
    int n = box_children(box, children);
    if (n == 0) return;

    if (n == 1 && _stricmp(children[0].class_name, "AureaCanvas") == 0) {
        SetWindowPos(children[0].hwnd, NULL, 0, 0, box_width, box_height,
                     SWP_NOZORDER | SWP_SHOWWINDOW);
        return;
    }

    int pad = box_padding(box);
    int spacing = ng_windows_px(box, SPACING);
    int cross = (vertical ? box_width : box_height) - 2 * pad;
    int main_room = (vertical ? box_height : box_width) - 2 * pad - spacing * (n - 1);
    float weights = 0.0f;
    for (int i = 0; i < n; i++) {
        measure_child(&children[i], vertical, cross);
        if (!vertical && children[i].weight > 0.0f && is_box(children[i].hwnd)) {
            children[i].w = 0;
        }
        main_room -= vertical ? children[i].h : children[i].w;
        weights += children[i].weight;
    }
    int last_takes_rest = weights == 0.0f &&
                          _stricmp(children[n - 1].class_name, "AureaCanvas") == 0;
    int free_room = main_room > 0 ? main_room : 0;

    int at = pad;
    for (int i = 0; i < n; i++) {
        BoxChild* c = &children[i];
        int extra = 0;
        if (weights > 0.0f && c->weight > 0.0f) {
            extra = (int)((float)free_room * c->weight / weights);
        } else if (last_takes_rest && i == n - 1) {
            extra = free_room;
        }
        int x, y, w, h;
        if (vertical) {
            x = pad;
            y = at;
            w = is_box(c->hwnd) || c->weight > 0.0f ? cross : c->w;
            h = c->h + extra;
            at += h + spacing;
        } else {
            x = at;
            w = c->w + extra;
            h = is_box(c->hwnd) || c->weight > 0.0f || c->h > cross ? cross : c->h;
            y = pad + (cross - h) / 2;
            at += w + spacing;
        }
        RECT was;
        GetClientRect(c->hwnd, &was);
        SetWindowPos(c->hwnd, NULL, x, y, w, h, SWP_NOZORDER | SWP_SHOWWINDOW);

        if (is_box(c->hwnd)) {
            if (was.right == w && was.bottom == h) layout_box_children(c->hwnd);
        } else if (_stricmp(c->class_name, "RichEdit20W") == 0) {
            SendMessageW(c->hwnd, EM_SETMARGINS, EC_LEFTMARGIN | EC_RIGHTMARGIN, MAKELONG(0, 0));
            SendMessageW(c->hwnd, EM_SETTARGETDEVICE, 0, 0);
        }
    }
}
