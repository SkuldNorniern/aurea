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
/* Control ids for buttons start here; menu item ids sit below it.

   Must stay inside 16 bits. WM_COMMAND carries the control id in LOWORD(wParam),
   so a base of 100000 truncated to 34465 and was mistaken for a menu id: every
   button click was dispatched to the menu registry and silently did nothing. */
#define BUTTON_COMMAND_BASE 0x8000

void layout_box_children(HWND box);
int get_box_orientation(HWND box);
void calculate_text_size(HDC hdc, const char* text, int* width, int* height);

#endif
