/* Boxes: measuring their children and laying them out in order, with weights. */

#include "common.h"
#include "../window_state.h"
#include "../utils.h"
#include "common/errors.h"
#include <commctrl.h>
#include <richedit.h>
#include <stdlib.h>
#include <string.h>

int get_box_orientation(HWND box) {
    return (INT_PTR)GetPropA(box, BOX_ORIENTATION_PROP) != BOX_HORIZONTAL;
}

int is_box(HWND hwnd) {
    return GetPropA(hwnd, BOX_ORIENTATION_PROP) != NULL;
}

static int box_setting(NGHandle handle, const char* prop, int value) {
    HWND box = (HWND)handle;
    if (!box || !IsWindow(box) || !is_box(box)) return NG_ERROR_INVALID_HANDLE;
    if (value < 0) return NG_ERROR_INVALID_PARAMETER;
    SetPropA(box, prop, (HANDLE)(INT_PTR)(value + 1));
    layout_box_children(box);
    ng_windows_relayout(box);
    return NG_SUCCESS;
}

int ng_windows_box_set_spacing(NGHandle box, int spacing) {
    return box_setting(box, BOX_SPACING_PROP, spacing);
}

int ng_windows_box_set_padding(NGHandle box, int padding) {
    return box_setting(box, BOX_PADDING_PROP, padding);
}

static int stored_px(HWND hwnd, const char* prop) {
    INT_PTR stored = (INT_PTR)GetPropA(hwnd, prop);
    return stored > 0 ? ng_windows_px(hwnd, (int)(stored - 1)) : -1;
}

void ng_windows_relayout(HWND hwnd) {
    HWND top = NULL;
    for (HWND at = GetParent(hwnd); at && is_box(at); at = GetParent(at)) top = at;
    if (top) layout_box_children(top);
    HWND outer = GetParent(top ? top : hwnd);
    if (ng_windows_is_scroll_view(outer)) ng_windows_scroll_view_layout(outer);
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
    int set = stored_px(box, BOX_PADDING_PROP);
    if (set >= 0) return set;
    HWND parent = GetParent(box);
    if (parent && (ng_windows_window_state(parent) ||
                   GetPropA(parent, "AureaSplitViewOldProc") || ng_windows_is_scroll_view(parent))) {
        return ng_windows_px(box, PADDING);
    }
    return 0;
}

static int box_spacing(HWND box) {
    int set = stored_px(box, BOX_SPACING_PROP);
    return set >= 0 ? set : ng_windows_px(box, SPACING);
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
        if (GetPropA(hwnd, BOX_LABEL_PROP)) flags |= DT_NOPREFIX;
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

static int is_line_edit(const BoxChild* c) {
    return _stricmp(c->class_name, "EDIT") == 0 &&
           !(GetWindowLongPtrA(c->hwnd, GWL_STYLE) & ES_MULTILINE);
}

/* Sized by their font; stretching them across a row only leaves the text hanging at the top. */
static int fixed_height(const BoxChild* c) {
    return is_line_edit(c) || _stricmp(c->class_name, "COMBOBOX") == 0;
}

static int line_edit_height(HWND hwnd) {
    HDC hdc = GetDC(hwnd);
    if (!hdc) return 0;
    HFONT old = (HFONT)SelectObject(hdc, control_font(hwnd));
    TEXTMETRICW tm = {0};
    GetTextMetricsW(hdc, &tm);
    SelectObject(hdc, old);
    ReleaseDC(hwnd, hdc);
    RECT outer, inner;
    GetWindowRect(hwnd, &outer);
    GetClientRect(hwnd, &inner);
    int border = (outer.bottom - outer.top) - inner.bottom;
    return tm.tmHeight + border + ng_windows_px(hwnd, LINE_EDIT_PADDING);
}

static void measure_box(HWND box, int in_vertical, int cross, int* out_w, int* out_h);

static void measure_child(BoxChild* c, int vertical, int cross) {
    RECT r;
    GetWindowRect(c->hwnd, &r);
    int w = r.right - r.left;
    int h = r.bottom - r.top;
    INT_PTR natural = (INT_PTR)GetPropA(c->hwnd, BOX_NATURAL_PROP);
    if (natural) {
        w = ng_windows_px(c->hwnd, (int)((natural >> 16) & 0xFFFF));
        h = ng_windows_px(c->hwnd, (int)(natural & 0xFFFF));
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
            h = empty ? 0 : text.cy;
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
            int padded = text.cx + ng_windows_px(c->hwnd, 32);
            int min_w = ng_windows_px(c->hwnd, BUTTON_MIN_WIDTH);
            int min_h = ng_windows_px(c->hwnd, BUTTON_MIN_HEIGHT);
            w = padded > min_w ? padded : min_w;
            if (h < min_h) h = min_h;
        }
    } else if (fixed_height(c)) {
        if (vertical && cross > 0) w = cross;
        h = is_line_edit(c) ? line_edit_height(c->hwnd) : r.bottom - r.top;
    } else if (_stricmp(cls, TRACKBAR_CLASSA) == 0 || _stricmp(cls, "msctls_progress32") == 0) {
        if (vertical && cross > 0) w = cross;
        h = ng_windows_px(c->hwnd, _stricmp(cls, TRACKBAR_CLASSA) == 0 ? SLIDER_HEIGHT : PROGRESS_HEIGHT);
    } else if (vertical && cross > 0 &&
               (c->weight > 0.0f || ng_windows_is_rich_edit(c->hwnd) || _stricmp(cls, "EDIT") == 0 ||
                _stricmp(cls, "AureaCanvas") == 0)) {
        w = cross;
    }
    int min_w = stored_px(c->hwnd, MIN_WIDTH_PROP);
    int min_h = stored_px(c->hwnd, MIN_HEIGHT_PROP);
    c->w = w > min_w ? w : min_w;
    c->h = h > min_h ? h : min_h;
}

static void measure_box(HWND box, int in_vertical, int cross, int* out_w, int* out_h) {
    BoxChild children[MAX_BOX_CHILDREN];
    int n = box_children(box, children);
    int vertical = get_box_orientation(box);
    int pad = box_padding(box);
    int spacing = box_spacing(box);
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

int ng_windows_measure_height(HWND hwnd, int width) {
    BoxChild c = {0};
    c.hwnd = hwnd;
    GetClassNameA(hwnd, c.class_name, sizeof(c.class_name));
    measure_child(&c, 1, width);
    return c.h;
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
        if (ng_windows_window_state(parent)) {
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
    int spacing = box_spacing(box);
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
            h = is_box(c->hwnd) || (c->weight > 0.0f && !fixed_height(c)) || c->h > cross
                    ? cross : c->h;
            y = pad + (cross - h) / 2;
            at += w + spacing;
        }
        RECT was;
        GetClientRect(c->hwnd, &was);
        /* Copied bits land on siblings not yet moved, and the next move carries them along. */
        SetWindowPos(c->hwnd, NULL, x, y, w, h, SWP_NOZORDER | SWP_SHOWWINDOW | SWP_NOCOPYBITS);

        if (is_box(c->hwnd)) {
            if (was.right == w && was.bottom == h) layout_box_children(c->hwnd);
        } else if (ng_windows_is_rich_edit(c->hwnd)) {
            int margin = ng_windows_px(c->hwnd, LABEL_PADDING);
            SendMessageW(c->hwnd, EM_SETMARGINS, EC_LEFTMARGIN | EC_RIGHTMARGIN, MAKELONG(margin, margin));
            SendMessageW(c->hwnd, EM_SETTARGETDEVICE, 0, GetPropA(c->hwnd, TEXT_NO_WRAP_PROP) ? 1 : 0);
        }
    }
}
