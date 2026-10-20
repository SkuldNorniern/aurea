#ifndef WINDOWS_ELEMENTS_COMMON_H
#define WINDOWS_ELEMENTS_COMMON_H

#include <windows.h>
#include <stdio.h>
#include "common/rust_callbacks.h"

#define LOG_ERROR(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_error(buf); \
} while(0)

#define LOG_WARN(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_warn(buf); \
} while(0)

#define LOG_INFO(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_info(buf); \
} while(0)

#define LOG_DEBUG(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_debug(buf); \
} while(0)

#define LOG_TRACE(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_trace(buf); \
} while(0)

#define PADDING 12
#define SPACING 8
#define BUTTON_MIN_WIDTH 80
#define BUTTON_MIN_HEIGHT 32
#define LABEL_PADDING 4
#define BOX_ORIENTATION_PROP "AureaBoxOrientation"
#define BOX_VERTICAL 2
#define BOX_HORIZONTAL 1
/* Set on each child when it is added: its place in the box, and its layout weight times 1000 plus 1, so that 0 still means unset. */
#define BOX_INDEX_PROP "AureaBoxIndex"
#define BOX_WEIGHT_PROP "AureaBoxWeight"
/* A child's size when it was added, width << 16 | height. */
#define BOX_NATURAL_PROP "AureaBoxNatural"
/* Set on labels, so layout can tell them from other STATIC windows. */
#define BOX_LABEL_PROP "AureaLabel"
/* Which callback a RichEdit's changes go to: text editor or text view. */
#define TEXT_CHANGE_EDITOR 1
#define TEXT_CHANGE_VIEW 2
/* Control ids for buttons start here; menu item ids sit below it. */
#define BUTTON_COMMAND_BASE 0x8000

void layout_box_children(HWND box);
int get_box_orientation(HWND box);
int is_box(HWND hwnd);
/* The font Windows uses for message boxes and dialogs, Segoe UI on current versions, at the DPI of the monitor `hwnd` is on. */
HFONT ng_windows_ui_font(HWND hwnd);
/* The DPI `hwnd` is drawn at, 96 when Windows cannot say. */
UINT ng_windows_dpi(HWND hwnd);
/* `logical` pixels in `hwnd`'s device pixels. */
int ng_windows_px(HWND hwnd, int logical);
/* Gives every control under `root` the UI font for its DPI and lays the content out again, after a DPI change or new content. */
void ng_windows_refresh_dpi(HWND root);
/* Where a control lives until it is added somewhere: one hidden window. */
#ifdef __cplusplus
extern "C"
#endif
HWND ng_windows_detached_parent(void);
void calculate_text_size(HDC hdc, const char* text, int* width, int* height);
/* A control under `parent` with UTF-8 `text`, through the wide API. */
HWND ng_windows_create_control(DWORD ex_style, const wchar_t* cls, const char* text,
                               DWORD style, int width, int height, HWND parent, UINT id);
/* Calls back with the text when the control's EN_CHANGE reaches the window. */
void ng_windows_watch_text(HWND edit, unsigned int id, int kind);
void ng_windows_text_changed(HWND edit);
/* Lays out the outermost box above `hwnd`, after its size may have changed. */
void ng_windows_relayout(HWND hwnd);
/* A tab bar's size; 0 when `bar` is not one. */
int ng_windows_tab_bar_measure(HWND bar, int* width, int* height);

#endif
