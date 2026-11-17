#import "pen.h"
#import "content_view.h"
#import "common/input.h"
#import "common/rust_callbacks.h"

enum { AUREA_PEN_ERASER = 1, AUREA_PEN_BARREL = 2 };

static BOOL g_pen_eraser = NO;

static BOOL is_pen_event(NSEvent* event) {
    switch (event.type) {
        case NSEventTypeTabletPoint:
            return YES;
        case NSEventTypeLeftMouseDown:
        case NSEventTypeLeftMouseUp:
        case NSEventTypeLeftMouseDragged:
        case NSEventTypeMouseMoved:
            return event.subtype == NSEventSubtypeTabletPoint;
        default:
            return NO;
    }
}

unsigned int ng_macos_pen_bits(NSEvent* event) {
    return event.subtype == NSEventSubtypeTabletPoint ? NG_POINTER_PEN : 0;
}

void ng_macos_pen_report(AureaContentView* view, NSEvent* event, unsigned int phase) {
    if (!view.windowHandle || !is_pen_event(event)) return;
    NSPoint p = [view convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [view bounds].size.height;
    NSPoint tilt = event.tilt;
    unsigned int flags = g_pen_eraser ? AUREA_PEN_ERASER : 0;
    if (event.buttonMask & (NSEventButtonMaskPenLowerSide | NSEventButtonMaskPenUpperSide)) {
        flags |= AUREA_PEN_BARREL;
    }
    double age = [[NSProcessInfo processInfo] systemUptime] - event.timestamp;
    ng_invoke_pen(view.windowHandle, p.x, h - p.y, event.pressure, (float)(tilt.x * 90.0),
                  (float)(tilt.y * 90.0), event.rotation, phase, flags, ng_macos_modifiers(event), age);
}

BOOL ng_macos_pen_proximity(NSEvent* event) {
    if (event.isEnteringProximity) {
        g_pen_eraser = event.pointingDeviceType == NSPointingDeviceTypeEraser;
    }
    return YES;
}

BOOL ng_macos_pen_point(AureaContentView* view, NSEvent* event) {
    ng_macos_pen_report(view, event, event.pressure > 0 ? AUREA_PEN_MOVE : AUREA_PEN_HOVER);
    return YES;
}
