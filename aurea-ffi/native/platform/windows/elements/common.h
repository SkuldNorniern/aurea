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
/* Control ids for buttons start here; menu item ids sit below it. */
#define BUTTON_COMMAND_BASE 0x8000

void layout_box_children(HWND box);
int get_box_orientation(HWND box);
int is_box(HWND hwnd);
/* Where a control lives until it is added somewhere: one hidden window. */
#ifdef __cplusplus
extern "C"
#endif
HWND ng_windows_detached_parent(void);
void calculate_text_size(HDC hdc, const char* text, int* width, int* height);

#endif
