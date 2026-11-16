/* DPI, the fonts controls draw with at each DPI, and refitting them when the DPI changes. */

#include "common.h"
#include "../utils.h"
#include "common/platform_api.h"
#include <richedit.h>
#include <stdlib.h>
#include <string.h>

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

typedef struct {
    LONG height;
    DWORD effects;
    COLORREF color;
    WCHAR face[LF_FACESIZE];
} RichBase;

#define RICH_BASE_PROP "AureaRichBase"
#define RICH_BASE_SUBCLASS_ID 0xF0

static LRESULT CALLBACK rich_base_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                       UINT_PTR id, DWORD_PTR data) {
    if (msg == WM_NCDESTROY) {
        free((RichBase*)data);
        RemovePropA(hwnd, RICH_BASE_PROP);
        ng_windows_remove_subclass(hwnd, rich_base_proc, id);
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

static void rich_default(HWND hwnd, CHARFORMAT2W* format) {
    memset(format, 0, sizeof(*format));
    format->cbSize = sizeof(*format);
    SendMessageW(hwnd, EM_GETCHARFORMAT, SCF_DEFAULT, (LPARAM)format);
}

#define RICH_WEIGHT (CFE_BOLD | CFE_ITALIC | CFE_UNDERLINE | CFE_STRIKEOUT)

void ng_windows_set_font(HWND hwnd, HFONT font, BOOL redraw, BOOL keep_format) {
    if (!ng_windows_is_rich_edit(hwnd)) {
        SendMessageW(hwnd, WM_SETFONT, (WPARAM)font, redraw);
        return;
    }
    /* WM_SETFONT resets every character to the font; what differs from the format Aurea set
       last time is the app's own, and goes back on after. */
    RichBase* base = (RichBase*)GetPropA(hwnd, RICH_BASE_PROP);
    CHARFORMAT2W now, keep = {0};
    keep.cbSize = sizeof(keep);
    rich_default(hwnd, &now);
    if (base && keep_format) {
        if (now.yHeight != base->height) {
            keep.dwMask |= CFM_SIZE;
            keep.yHeight = now.yHeight;
        }
        if (wcscmp(now.szFaceName, base->face) != 0) {
            keep.dwMask |= CFM_FACE | CFM_CHARSET;
            keep.bCharSet = now.bCharSet;
            keep.bPitchAndFamily = now.bPitchAndFamily;
            wcscpy_s(keep.szFaceName, LF_FACESIZE, now.szFaceName);
        }
        if (!(now.dwEffects & CFE_AUTOCOLOR) &&
            ((base->effects & CFE_AUTOCOLOR) || now.crTextColor != base->color)) {
            keep.dwMask |= CFM_COLOR;
            keep.crTextColor = now.crTextColor;
        }
        if ((now.dwEffects & RICH_WEIGHT) != (base->effects & RICH_WEIGHT)) {
            keep.dwMask |= CFM_BOLD | CFM_ITALIC | CFM_UNDERLINE | CFM_STRIKEOUT;
            keep.dwEffects |= now.dwEffects & RICH_WEIGHT;
        }
    }
    SendMessageW(hwnd, WM_SETFONT, (WPARAM)font, keep.dwMask ? FALSE : redraw);
    if (!base) {
        base = (RichBase*)calloc(1, sizeof(RichBase));
        if (base && !ng_windows_set_subclass(hwnd, rich_base_proc, RICH_BASE_SUBCLASS_ID, (DWORD_PTR)base)) {
            free(base);
            base = NULL;
        }
        if (base) SetPropA(hwnd, RICH_BASE_PROP, (HANDLE)base);
    }
    if (base) {
        CHARFORMAT2W fresh;
        rich_default(hwnd, &fresh);
        base->height = fresh.yHeight;
        base->effects = fresh.dwEffects;
        base->color = fresh.crTextColor;
        wcscpy_s(base->face, LF_FACESIZE, fresh.szFaceName);
    }
    if (keep.dwMask) {
        SendMessageW(hwnd, EM_SETCHARFORMAT, SCF_DEFAULT, (LPARAM)&keep);
        SendMessageW(hwnd, EM_SETCHARFORMAT, SCF_ALL, (LPARAM)&keep);
        if (redraw) InvalidateRect(hwnd, NULL, TRUE);
    }
}

static BOOL CALLBACK ng_windows_refont_child(HWND child, LPARAM dpi) {
    HFONT current = (HFONT)SendMessageW(child, WM_GETFONT, 0, 0);
    int kind = current ? ng_windows_font_kind(current) : -1;
    if (kind < 0) return TRUE;
    HFONT font = ng_windows_font(kind, (UINT)dpi);
    if (font != current) ng_windows_set_font(child, font, TRUE, TRUE);
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
