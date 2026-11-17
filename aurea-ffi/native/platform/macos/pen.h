#ifndef NATIVE_GUI_MACOS_PEN_H
#define NATIVE_GUI_MACOS_PEN_H

#import <Cocoa/Cocoa.h>

@class AureaContentView;

enum { AUREA_PEN_HOVER = 0, AUREA_PEN_DOWN = 1, AUREA_PEN_MOVE = 2, AUREA_PEN_UP = 3 };

/* NG_POINTER_PEN when a tablet moved the pointer for `event`, else 0. */
unsigned int ng_macos_pen_bits(NSEvent* event);
/* Reports `event` as a pen sample when a tablet sent it; `phase` is one of AUREA_PEN_*. */
void ng_macos_pen_report(AureaContentView* view, NSEvent* event, unsigned int phase);
/* A tablet pen came near or left, YES when taken. */
BOOL ng_macos_pen_proximity(NSEvent* event);
/* A tablet sample between mouse events, YES when taken. */
BOOL ng_macos_pen_point(AureaContentView* view, NSEvent* event);

#endif
