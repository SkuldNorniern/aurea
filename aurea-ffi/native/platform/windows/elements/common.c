#include "common.h"
#include "../elements.h"
#include "../utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include "common/platform_api.h"
#include <commctrl.h>
#include <initguid.h>
#include <oleacc.h>
#include <richedit.h>
#include <stdlib.h>
#include <string.h>

int get_box_orientation(HWND box) {
    return (INT_PTR)GetPropA(box, BOX_ORIENTATION_PROP) != BOX_HORIZONTAL;
}

static WNDPROC g_parking_proc = NULL;

static LRESULT CALLBACK parking_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    HWND from = NULL;
    if (msg == WM_COMMAND || msg == WM_HSCROLL || msg == WM_VSCROLL) from = (HWND)lParam;
    else if (msg == WM_NOTIFY && lParam) from = ((NMHDR*)lParam)->hwndFrom;
    else if (msg >= WM_CTLCOLORMSGBOX && msg <= WM_CTLCOLORSTATIC) from = (HWND)lParam;
    if (from) {
        HWND parent = GetParent(from);
        if (parent && parent != hwnd) return SendMessageW(parent, msg, wParam, lParam);
    }
    return CallWindowProcA(g_parking_proc, hwnd, msg, wParam, lParam);
}

NGHandle ng_windows_creation_parent(void) {
    return (NGHandle)ng_windows_detached_parent();
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
enum { FONT_UI, FONT_STRONG, FONT_SMALL, FONT_HEADING, FONT_MONO, FONT_KINDS };
static struct { UINT dpi; HFONT font; } g_fonts[FONT_KINDS][AUREA_FONT_SLOTS];

static HFONT ng_windows_make_font(int kind, UINT dpi) {
    if (kind == FONT_MONO) {
        HFONT mono = CreateFontW(-MulDiv(10, (int)dpi, 72), 0, 0, 0, FW_NORMAL, FALSE, FALSE,
                                 FALSE, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                                 CLEARTYPE_QUALITY, FIXED_PITCH | FF_MODERN, L"Consolas");
        return mono ? mono : (HFONT)GetStockObject(ANSI_FIXED_FONT);
    }
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
    LOGFONTW lf = metrics.lfMessageFont;
    if (kind == FONT_STRONG || kind == FONT_HEADING) lf.lfWeight = FW_SEMIBOLD;
    if (kind == FONT_SMALL) lf.lfHeight = MulDiv(lf.lfHeight, 9, 10);
    if (kind == FONT_HEADING) lf.lfHeight = MulDiv(lf.lfHeight, 7, 5);
    HFONT font = CreateFontIndirectW(&lf);
    return font ? font : (HFONT)GetStockObject(DEFAULT_GUI_FONT);
}

static HFONT ng_windows_font(int kind, UINT dpi) {
    for (int i = 0; i < AUREA_FONT_SLOTS; i++) {
        if (g_fonts[kind][i].font && g_fonts[kind][i].dpi == dpi) return g_fonts[kind][i].font;
    }
    int slot = AUREA_FONT_SLOTS - 1;
    for (int i = 0; i < AUREA_FONT_SLOTS; i++) {
        if (!g_fonts[kind][i].font) {
            slot = i;
            break;
        }
    }
    g_fonts[kind][slot].dpi = dpi;
    g_fonts[kind][slot].font = ng_windows_make_font(kind, dpi);
    return g_fonts[kind][slot].font;
}

HFONT ng_windows_ui_font(HWND hwnd) {
    return ng_windows_font(FONT_UI, ng_windows_dpi(hwnd));
}

HFONT ng_windows_style_font(HWND hwnd, int style) {
    int kind = style == NG_TEXT_STYLE_STRONG ? FONT_STRONG
             : style == NG_TEXT_STYLE_SMALL  ? FONT_SMALL
             : style == NG_TEXT_STYLE_HEADING ? FONT_HEADING
                                              : FONT_UI;
    return ng_windows_font(kind, ng_windows_dpi(hwnd));
}

HFONT ng_windows_mono_font(HWND hwnd) {
    return ng_windows_font(FONT_MONO, ng_windows_dpi(hwnd));
}

static int ng_windows_font_kind(HFONT font) {
    for (int kind = 0; kind < FONT_KINDS; kind++) {
        for (int i = 0; i < AUREA_FONT_SLOTS; i++) {
            if (g_fonts[kind][i].font && g_fonts[kind][i].font == font) return kind;
        }
    }
    return -1;
}

static BOOL CALLBACK ng_windows_refont_child(HWND child, LPARAM dpi) {
    HFONT current = (HFONT)SendMessageW(child, WM_GETFONT, 0, 0);
    int kind = current ? ng_windows_font_kind(current) : -1;
    if (kind < 0) return TRUE;
    HFONT font = ng_windows_font(kind, (UINT)dpi);
    if (font != current) SendMessageW(child, WM_SETFONT, (WPARAM)font, TRUE);
    return TRUE;
}

static BOOL CALLBACK ng_windows_relayout_child(HWND child, LPARAM unused) {
    (void)unused;
    if (is_box(child) && !is_box(GetParent(child))) layout_box_children(child);
    else if (ng_windows_is_scroll_view(child)) ng_windows_scroll_view_layout(child);
    return TRUE;
}

void ng_windows_refresh_dpi(HWND root) {
    if (!root) return;
    EnumChildWindows(root, ng_windows_refont_child, (LPARAM)ng_windows_dpi(root));
    EnumChildWindows(root, ng_windows_relayout_child, 0);
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

#define CHANGE_ID_PROP "AureaChangeId"

int ng_windows_is_notification(UINT msg) {
    return msg == WM_COMMAND || msg == WM_NOTIFY || msg == WM_HSCROLL || msg == WM_VSCROLL;
}

void ng_windows_forward_to_window(HWND from, UINT msg, WPARAM wParam, LPARAM lParam) {
    for (HWND at = GetParent(from); at; at = GetParent(at)) {
        char cls[64];
        if (GetClassNameA(at, cls, sizeof(cls)) && _stricmp(cls, "NativeGuiWindow") == 0) {
            SendMessageW(at, msg, wParam, lParam);
            return;
        }
    }
}

int ng_windows_element_set_change_id(NGHandle element, unsigned int id) {
    if (!element || !IsWindow((HWND)element)) return NG_ERROR_INVALID_HANDLE;
    HWND hwnd = (HWND)element;
    SetPropA(hwnd, CHANGE_ID_PROP, (HANDLE)(UINT_PTR)id);
    wchar_t cls[32];
    if (ng_windows_is_rich_edit(hwnd)) {
        LRESULT mask = SendMessageW(hwnd, EM_GETEVENTMASK, 0, 0);
        SendMessageW(hwnd, EM_SETEVENTMASK, 0, mask | ENM_CHANGE);
    }
    return NG_SUCCESS;
}

int ng_windows_control_changed(UINT msg, WPARAM wParam, LPARAM lParam) {
    HWND control = (HWND)lParam;
    if (!control) return 0;
    unsigned int id = (unsigned int)(UINT_PTR)GetPropA(control, CHANGE_ID_PROP);
    if (!id) return 0;
    int changed = 0;
    if (msg == WM_COMMAND && HIWORD(wParam) == EN_CHANGE) {
        if (GetPropA(control, TEXT_QUIET_PROP)) return 1;
        changed = 1;
    } else if (msg == WM_COMMAND) {
        changed = HIWORD(wParam) == BN_CLICKED || HIWORD(wParam) == CBN_SELCHANGE;
    } else if (msg == WM_HSCROLL || msg == WM_VSCROLL) {
        changed = LOWORD(wParam) != TB_ENDTRACK;
    }
    if (changed) ng_invoke_custom_callback(id);
    return changed;
}

static BOOL CALLBACK enable_child(HWND child, LPARAM enabled) {
    EnableWindow(child, (BOOL)enabled);
    return TRUE;
}

int ng_windows_element_set_enabled(NGHandle element, int enabled) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    EnableWindow(hwnd, enabled ? TRUE : FALSE);
    EnumChildWindows(hwnd, enable_child, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

static LRESULT CALLBACK destroy_watch_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                           UINT_PTR subclass, DWORD_PTR id) {
    if (msg == WM_NCDESTROY) {
        ng_windows_remove_subclass(hwnd, destroy_watch_proc, subclass);
        ng_invoke_custom_callback((unsigned int)id);
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

int ng_windows_element_watch_destroy(NGHandle element, unsigned int id) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    return ng_windows_set_subclass(hwnd, destroy_watch_proc, id, id) ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_windows_element_set_visible(NGHandle element, int visible) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    ShowWindow(hwnd, visible ? SW_SHOW : SW_HIDE);
    ng_windows_relayout(hwnd);
    return NG_SUCCESS;
}

int ng_windows_element_set_min_size(NGHandle element, int width, int height) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (width < 0 || height < 0) return NG_ERROR_INVALID_PARAMETER;
    SetPropA(hwnd, MIN_WIDTH_PROP, (HANDLE)(INT_PTR)(width + 1));
    SetPropA(hwnd, MIN_HEIGHT_PROP, (HANDLE)(INT_PTR)(height + 1));
    ng_windows_relayout(hwnd);
    return NG_SUCCESS;
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

static IAccPropServices* acc_props(void) {
    static IAccPropServices* props;
    static int tried;
    if (!tried) {
        tried = 1;
        HRESULT init = CoInitializeEx(NULL, COINIT_APARTMENTTHREADED);
        if (SUCCEEDED(init) || init == RPC_E_CHANGED_MODE) {
            CoCreateInstance(&CLSID_AccPropServices, NULL, CLSCTX_INPROC_SERVER,
                             &IID_IAccPropServices, (void**)&props);
        }
    }
    return props;
}

#define VALUE_SERVER_PROP "AureaAccValue"
#define STATE_SERVER_PROP "AureaAccState"
#define ACCESSIBLE_SUBCLASS_ID 0xACC

/* Annotations outlive their window; a later window given the same handle would inherit them. */
static void forget_accessible(HWND hwnd) {
    RemovePropA(hwnd, VALUE_SERVER_PROP);
    RemovePropA(hwnd, STATE_SERVER_PROP);
    IAccPropServices* props = acc_props();
    if (!props) return;
    MSAAPROPID all[] = {PROPID_ACC_NAME, PROPID_ACC_ROLE, PROPID_ACC_VALUE, PROPID_ACC_STATE};
    props->lpVtbl->ClearHwndProps(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF, all, 4);
}

static LRESULT CALLBACK accessible_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                        UINT_PTR id, DWORD_PTR data) {
    (void)data;
    if (msg == WM_NCDESTROY) {
        forget_accessible(hwnd);
        ng_windows_remove_subclass(hwnd, accessible_proc, id);
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

static IAccPropServices* annotate(HWND hwnd) {
    IAccPropServices* props = acc_props();
    if (props) ng_windows_set_subclass(hwnd, accessible_proc, ACCESSIBLE_SUBCLASS_ID, 0);
    return props;
}

void ng_windows_set_role(HWND hwnd, long role) {
    IAccPropServices* props = annotate(hwnd);
    if (!props) return;
    VARIANT value;
    VariantInit(&value);
    value.vt = VT_I4;
    value.lVal = role;
    props->lpVtbl->SetHwndProp(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF, PROPID_ACC_ROLE, value);
}

/* Layout containers are STATIC windows; without this a screen reader stops on each as empty text. */
void ng_windows_mark_container(HWND hwnd) {
    ng_windows_set_role(hwnd, ROLE_SYSTEM_PANE);
}

static HRESULT STDMETHODCALLTYPE server_query(IAccPropServer* self, REFIID iid, void** out) {
    if (IsEqualIID(iid, &IID_IUnknown) || IsEqualIID(iid, &IID_IAccPropServer)) {
        *out = self;
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}

static ULONG STDMETHODCALLTYPE server_ref(IAccPropServer* self) {
    (void)self;
    return 1;
}

static HRESULT STDMETHODCALLTYPE server_get(IAccPropServer* self, const BYTE* id, DWORD id_len,
                                            MSAAPROPID prop, VARIANT* out, BOOL* has) {
    (void)self;
    VariantInit(out);
    *has = FALSE;
    IAccPropServices* props = acc_props();
    HWND hwnd = NULL;
    DWORD object = 0, child = 0;
    if (!props || FAILED(props->lpVtbl->DecomposeHwndIdentityString(props, id, id_len, &hwnd,
                                                                    &object, &child))) {
        return S_OK;
    }
    if (IsEqualGUID(&prop, &PROPID_ACC_VALUE)) {
        int (*value)(HWND, wchar_t*, int) = (int (*)(HWND, wchar_t*, int))GetPropA(hwnd, VALUE_SERVER_PROP);
        wchar_t text[64];
        if (!value || !value(hwnd, text, 64)) return S_OK;
        out->vt = VT_BSTR;
        out->bstrVal = SysAllocString(text);
        *has = out->bstrVal != NULL;
    } else if (IsEqualGUID(&prop, &PROPID_ACC_STATE)) {
        long (*state)(HWND) = (long (*)(HWND))GetPropA(hwnd, STATE_SERVER_PROP);
        if (!state) return S_OK;
        out->vt = VT_I4;
        out->lVal = state(hwnd);
        *has = TRUE;
    }
    return S_OK;
}

static IAccPropServerVtbl g_server_vtbl = {server_query, server_ref, server_ref, server_get};
static IAccPropServer g_server = {&g_server_vtbl};

static void serve(HWND hwnd, const char* name, void* callback, MSAAPROPID prop) {
    IAccPropServices* props = annotate(hwnd);
    if (!props) return;
    SetPropA(hwnd, name, (HANDLE)callback);
    props->lpVtbl->SetHwndPropServer(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF, &prop, 1,
                                     &g_server, ANNO_THIS);
}

void ng_windows_serve_value(HWND hwnd, int (*value)(HWND hwnd, wchar_t* out, int len)) {
    serve(hwnd, VALUE_SERVER_PROP, (void*)value, PROPID_ACC_VALUE);
}

void ng_windows_serve_state(HWND hwnd, long (*state)(HWND hwnd)) {
    serve(hwnd, STATE_SERVER_PROP, (void*)state, PROPID_ACC_STATE);
}

int ng_windows_element_set_accessible_name(NGHandle element, const char* name) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (!name) return NG_ERROR_INVALID_PARAMETER;
    IAccPropServices* props = annotate(hwnd);
    if (!props) return NG_ERROR_PLATFORM_SPECIFIC;
    if (!*name) {
        MSAAPROPID prop = PROPID_ACC_NAME;
        props->lpVtbl->ClearHwndProps(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF, &prop, 1);
        return NG_SUCCESS;
    }
    wchar_t* wide = ng_windows_utf8_to_wide(name);
    if (!wide) return NG_ERROR_INVALID_PARAMETER;
    HRESULT set = props->lpVtbl->SetHwndPropStr(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF,
                                                PROPID_ACC_NAME, wide);
    free(wide);
    return SUCCEEDED(set) ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

void ng_windows_unlimit_text(HWND edit) {
    SendMessageW(edit, EM_EXLIMITTEXT, 0, 0x7FFFFFFE);
}

void ng_windows_relayout(HWND hwnd) {
    HWND top = NULL;
    for (HWND at = GetParent(hwnd); at && is_box(at); at = GetParent(at)) top = at;
    if (top) layout_box_children(top);
    HWND outer = GetParent(top ? top : hwnd);
    if (ng_windows_is_scroll_view(outer)) ng_windows_scroll_view_layout(outer);
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
    int set = stored_px(box, BOX_PADDING_PROP);
    if (set >= 0) return set;
    HWND parent = GetParent(box);
    char parent_class[64];
    if (parent && ((GetClassNameA(parent, parent_class, sizeof(parent_class)) &&
                    _stricmp(parent_class, "NativeGuiWindow") == 0) ||
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
            int natural = text.cx + ng_windows_px(c->hwnd, 32);
            int min_w = ng_windows_px(c->hwnd, BUTTON_MIN_WIDTH);
            int min_h = ng_windows_px(c->hwnd, BUTTON_MIN_HEIGHT);
            w = natural > min_w ? natural : min_w;
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
        SetWindowPos(c->hwnd, NULL, x, y, w, h, SWP_NOZORDER | SWP_SHOWWINDOW);

        if (is_box(c->hwnd)) {
            if (was.right == w && was.bottom == h) layout_box_children(c->hwnd);
        } else if (ng_windows_is_rich_edit(c->hwnd)) {
            int margin = ng_windows_px(c->hwnd, LABEL_PADDING);
            SendMessageW(c->hwnd, EM_SETMARGINS, EC_LEFTMARGIN | EC_RIGHTMARGIN, MAKELONG(margin, margin));
            SendMessageW(c->hwnd, EM_SETTARGETDEVICE, 0, GetPropA(c->hwnd, TEXT_NO_WRAP_PROP) ? 1 : 0);
        }
    }
}
