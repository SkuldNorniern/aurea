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
#define LINE_EDIT_PADDING 4
/* Leave room around the native thumb; at 22px it is clipped by one pixel. */
#define SLIDER_HEIGHT 24
#define PROGRESS_HEIGHT 20
#define BOX_ORIENTATION_PROP "AureaBoxOrientation"
#define BOX_VERTICAL 2
#define BOX_HORIZONTAL 1
/* Set on each child when it is added: its place in the box, and its layout weight times 1000 plus 1, so that 0 still means unset. */
#define BOX_INDEX_PROP "AureaBoxIndex"
#define BOX_WEIGHT_PROP "AureaBoxWeight"
/* A child's logical size when it was added, width << 16 | height. */
#define BOX_NATURAL_PROP "AureaBoxNatural"
/* Set on labels, so layout can tell them from other STATIC windows. */
#define BOX_LABEL_PROP "AureaLabel"
/* On the containers of a sidebar list and a table; the list inside is what a screen reader reads. */
#define SIDEBAR_DATA_PROP "AureaSidebarData"
#define TABLE_DATA_PROP "AureaTableData"
/* Set while Aurea changes a text control, so the change is not the user's. */
#define TEXT_QUIET_PROP "AureaTextQuiet"
/* Set on a RichEdit whose lines run on instead of wrapping. */
#define TEXT_NO_WRAP_PROP "AureaTextNoWrap"
/* Logical sizes stored plus 1, so 0 still means unset. */
#define MIN_WIDTH_PROP "AureaMinWidth"
#define MIN_HEIGHT_PROP "AureaMinHeight"
#define BOX_SPACING_PROP "AureaBoxSpacing"
#define BOX_PADDING_PROP "AureaBoxPadding"
#define SCROLL_VIEW_OLD_PROC_PROP "AureaScrollViewOldProc"
/* Control ids for buttons start here; menu item ids sit below it. */
#define BUTTON_COMMAND_BASE 0x8000

void layout_box_children(HWND box);
int get_box_orientation(HWND box);
int is_box(HWND hwnd);
/* The font Windows uses for message boxes and dialogs, Segoe UI on current versions, at the DPI of the monitor `hwnd` is on. */
HFONT ng_windows_ui_font(HWND hwnd);
/* WM_SETFONT, except a RichEdit keeps the face, size, colour and weight the app gave it since. */
void ng_windows_set_font(HWND hwnd, HFONT font, BOOL redraw, BOOL keep_format);
/* A fixed-width face for source text, at the DPI `hwnd` is on, kept up to date the same way. */
HFONT ng_windows_mono_font(HWND hwnd);
/* The UI font in one of the NG_TEXT_STYLE_* styles, at the DPI `hwnd` is on. */
HFONT ng_windows_style_font(HWND hwnd, int style);
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
/* A multi-line RichEdit, the base of text views and editors. */
HWND ng_windows_create_rich_text(int editable);
int ng_windows_is_rich_edit(HWND hwnd);
/* Lays out the outermost box above `hwnd`, after its size may have changed. */
void ng_windows_relayout(HWND hwnd);
/* RichEdit stops typing at 32K characters unless told otherwise. */
void ng_windows_unlimit_text(HWND edit);
/* What screen readers are told in place of the native control's own role, value or state. */
/* 0 gives the native role back. */
void ng_windows_set_role(HWND hwnd, long role);
void ng_windows_mark_container(HWND hwnd);
void ng_windows_serve_value(HWND hwnd, int (*value)(HWND hwnd, wchar_t* out, int len));
void ng_windows_serve_state(HWND hwnd, long (*state)(HWND hwnd));
/* A text control's content as UTF-8 with " " line breaks; the caller frees it. */
char* ng_windows_text_content(HWND hwnd);
/* Messages a control sends its parent, which containers pass up. */
int ng_windows_is_notification(UINT msg);
void ng_windows_forward_to_window(HWND from, UINT msg, WPARAM wParam, LPARAM lParam);
/* For the window's procedure: a control's notification, 1 if it was a change. */
int ng_windows_control_changed(UINT msg, WPARAM wParam, LPARAM lParam);
/* A tab bar's size; 0 when `bar` is not one. */
int ng_windows_tab_bar_measure(HWND bar, int* width, int* height);
/* The height `hwnd` wants at `width`. */
int ng_windows_measure_height(HWND hwnd, int width);
int ng_windows_is_scroll_view(HWND hwnd);
/* Sizes the content to its height and updates the scroll range. */
void ng_windows_scroll_view_layout(HWND scroll);
/* Pixels to scroll for a wheel `delta`; `rest` carries the remainder between events. */
int ng_windows_wheel_step(int delta, unsigned int lines, int line_px, int page, int* rest);

#endif
