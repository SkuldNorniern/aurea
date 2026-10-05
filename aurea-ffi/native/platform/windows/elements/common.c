#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <richedit.h>
#include <string.h>

int get_box_orientation(HWND box) {
    /* Unset reads as vertical, as it always has. Horizontal is stored as 1,
       not 0, because a property of 0 cannot be told from no property. */
    return (INT_PTR)GetPropA(box, BOX_ORIENTATION_PROP) != BOX_HORIZONTAL;
}

HWND ng_windows_detached_parent(void) {
    static HWND parking = NULL;
    if (!parking || !IsWindow(parking)) {
        parking = CreateWindowExA(WS_EX_TOOLWINDOW, "STATIC", NULL, WS_POPUP,
                                  0, 0, 0, 0, NULL, NULL, GetModuleHandleA(NULL), NULL);
    }
    return parking ? parking : GetDesktopWindow();
}

HFONT ng_windows_ui_font(void) {
    static HFONT font = NULL;
    if (!font) {
        NONCLIENTMETRICSW metrics;
        metrics.cbSize = sizeof(metrics);
        if (SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, sizeof(metrics), &metrics, 0)) {
            font = CreateFontIndirectW(&metrics.lfMessageFont);
        }
        if (!font) font = (HFONT)GetStockObject(DEFAULT_GUI_FONT);
    }
    return font;
}

int is_box(HWND hwnd) {
    return GetPropA(hwnd, BOX_ORIENTATION_PROP) != NULL;
}

void calculate_text_size(HDC hdc, const char* text, int* width, int* height) {
    LOG_TRACE("calculate_text_size: called with text='%s'", text ? text : "(null)");
    
    if (!hdc || !text || !width || !height) {
        LOG_WARN("calculate_text_size: Invalid parameters");
        return;
    }
    
    SIZE text_size;
    int text_len = (int)strlen(text);
    
    if (GetTextExtentPoint32A(hdc, text, text_len, &text_size)) {
        *width = text_size.cx;
        *height = text_size.cy;
    } else {
        *width = 100;
        *height = 20;
    }
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

/* The top-level box keeps a margin from the window edge; nested boxes sit
   flush in theirs, or the margins would add up. */
static int box_padding(HWND box) {
    HWND parent = GetParent(box);
    char parent_class[64];
    if (parent && GetClassNameA(parent, parent_class, sizeof(parent_class)) &&
        _stricmp(parent_class, "NativeGuiWindow") == 0) {
        return PADDING;
    }
    return 0;
}

/* A box's children in the order they were added. The z-order cannot be used:
   SetParent puts each new child on top, which reverses it. */
static int box_children(HWND box, BoxChild* out) {
    int n = 0;
    for (HWND child = GetWindow(box, GW_CHILD); child && n < MAX_BOX_CHILDREN;
         child = GetWindow(child, GW_HWNDNEXT)) {
        /* The style bit, not IsWindowVisible: that is false for every child
           until the window itself is shown. */
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
    HFONT font = (HFONT)SendMessageA(hwnd, WM_GETFONT, 0, 0);
    /* A control with no font draws in the old System font. */
    return font ? font : (HFONT)GetStockObject(SYSTEM_FONT);
}

/* Size of a control's text, wrapped to `wrap` pixels when it is above 0. */
static SIZE text_extent(HWND hwnd, int wrap) {
    SIZE size = {0, 0};
    char text[1024];
    int len = GetWindowTextA(hwnd, text, sizeof(text));
    HDC hdc = GetDC(hwnd);
    if (!hdc) return size;
    HFONT old = (HFONT)SelectObject(hdc, control_font(hwnd));
    if (len > 0) {
        RECT r = {0, 0, wrap > 0 ? wrap : 0, 0};
        UINT flags = DT_LEFT | DT_CALCRECT | (wrap > 0 ? DT_WORDBREAK : DT_SINGLELINE);
        DrawTextA(hdc, text, len, &r, flags);
        size.cx = r.right;
        size.cy = r.bottom;
    } else {
        TEXTMETRICA tm;
        GetTextMetricsA(hdc, &tm);
        size.cy = tm.tmHeight;
    }
    SelectObject(hdc, old);
    ReleaseDC(hwnd, hdc);
    return size;
}

static void measure_box(HWND box, int in_vertical, int cross, int* out_w, int* out_h);

/* What a child wants. `cross` is the room across the box: the width in a
   vertical box, the height in a horizontal one, or -1 when not known. */
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
    } else if (_stricmp(cls, "STATIC") == 0) {
        LONG_PTR style = GetWindowLongPtrA(c->hwnd, GWL_STYLE);
        if ((style & SS_TYPEMASK) == SS_LEFTNOWORDWRAP) {
            SetWindowLongPtrA(c->hwnd, GWL_STYLE, (style & ~SS_TYPEMASK) | SS_LEFT);
        }
        int empty = GetWindowTextLengthA(c->hwnd) == 0;
        if (vertical) {
            SIZE text = text_extent(c->hwnd, cross > 0 ? cross : 0);
            w = cross > 0 ? cross : text.cx;
            h = empty ? 0 : text.cy + LABEL_PADDING * 2;
        } else {
            SIZE text = text_extent(c->hwnd, 0);
            w = empty ? 0 : text.cx + 2;
            h = empty ? 0 : text.cy + LABEL_PADDING * 2;
        }
    } else if (_stricmp(cls, "BUTTON") == 0) {
        LONG_PTR type = GetWindowLongPtrA(c->hwnd, GWL_STYLE) & BS_TYPEMASK;
        SIZE text = text_extent(c->hwnd, 0);
        if (type == BS_AUTOCHECKBOX || type == BS_CHECKBOX || type == BS_AUTORADIOBUTTON ||
            type == BS_RADIOBUTTON || type == BS_AUTO3STATE || type == BS_3STATE) {
            w = vertical && cross > 0 ? cross : text.cx + 24;
            h = text.cy + 8 > 20 ? text.cy + 8 : 20;
        } else {
            int natural = text.cx + 32;
            w = natural > BUTTON_MIN_WIDTH ? natural : BUTTON_MIN_WIDTH;
            if (h < BUTTON_MIN_HEIGHT) h = BUTTON_MIN_HEIGHT;
        }
    } else if (vertical && cross > 0 &&
               (_stricmp(cls, "RichEdit20A") == 0 || _stricmp(cls, "EDIT") == 0 ||
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
    /* A box fills the width of a vertical box it sits in. */
    int width = in_vertical && cross > 0 ? cross : -1;
    int main = 0, across = 0;
    for (int i = 0; i < n; i++) {
        measure_child(&children[i], vertical,
                      vertical && width > 0 ? width - 2 * pad : -1);
        int m = vertical ? children[i].h : children[i].w;
        int a = vertical ? children[i].w : children[i].h;
        main += m + (i > 0 ? SPACING : 0);
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

    /* A canvas alone in a box fills it edge to edge. */
    if (n == 1 && _stricmp(children[0].class_name, "AureaCanvas") == 0) {
        SetWindowPos(children[0].hwnd, NULL, 0, 0, box_width, box_height,
                     SWP_NOZORDER | SWP_SHOWWINDOW);
        return;
    }

    int pad = box_padding(box);
    int cross = (vertical ? box_width : box_height) - 2 * pad;
    int main_room = (vertical ? box_height : box_width) - 2 * pad - SPACING * (n - 1);
    float weights = 0.0f;
    for (int i = 0; i < n; i++) {
        measure_child(&children[i], vertical, cross);
        /* A weighted box takes its share of the room, not its natural
           length: in a row, that length is its text unwrapped, which
           would push it past the edge. */
        if (!vertical && children[i].weight > 0.0f && is_box(children[i].hwnd)) {
            children[i].w = 0;
        }
        main_room -= vertical ? children[i].h : children[i].w;
        weights += children[i].weight;
    }
    /* Without weights, a canvas at the end takes what is left, as before. */
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
            w = is_box(c->hwnd) ? cross : c->w;
            h = c->h + extra;
            at += h + SPACING;
        } else {
            x = at;
            w = c->w + extra;
            h = is_box(c->hwnd) || c->h > cross ? cross : c->h;
            /* Row items sit on the row's middle. */
            y = pad + (cross - h) / 2;
            at += w + SPACING;
        }
        SetWindowPos(c->hwnd, NULL, x, y, w, h, SWP_NOZORDER | SWP_SHOWWINDOW);

        if (is_box(c->hwnd)) {
            layout_box_children(c->hwnd);
        } else if (_stricmp(c->class_name, "RichEdit20A") == 0) {
            SendMessageA(c->hwnd, EM_SETMARGINS, EC_LEFTMARGIN | EC_RIGHTMARGIN, MAKELONG(0, 0));
            /* Wrap at the control's width. The width argument is in twips,
               so pixels there wrapped after a few characters. */
            SendMessageA(c->hwnd, EM_SETTARGETDEVICE, 0, 0);
        }
    }
}
