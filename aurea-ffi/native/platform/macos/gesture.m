#import "gesture.h"
#import "content_view.h"
#import "common/input.h"
#import "common/rust_callbacks.h"
#include <math.h>

BOOL ng_macos_gesture_magnify(AureaContentView* view, NSEvent* event) {
    if (!view.windowHandle || event.magnification == 0.0) return YES;
    NSPoint p = [view convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [view bounds].size.height;
    ng_invoke_gesture(view.windowHandle, NG_GESTURE_MAGNIFY, 1.0 + (double)event.magnification, p.x, h - p.y);
    return YES;
}

BOOL ng_macos_gesture_rotate(AureaContentView* view, NSEvent* event) {
    if (!view.windowHandle || event.rotation == 0.0) return YES;
    NSPoint p = [view convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [view bounds].size.height;
    /* Apple reports degrees counter-clockwise; Rotate wants radians clockwise. */
    ng_invoke_gesture(view.windowHandle, NG_GESTURE_ROTATE, -(double)event.rotation * M_PI / 180.0, p.x, h - p.y);
    return YES;
}
