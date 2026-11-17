#import "gesture.h"

/* Without gestures pinch and twist go on up the responder chain. */
BOOL ng_macos_gesture_magnify(AureaContentView* view, NSEvent* event) {
    (void)view;
    (void)event;
    return NO;
}

BOOL ng_macos_gesture_rotate(AureaContentView* view, NSEvent* event) {
    (void)view;
    (void)event;
    return NO;
}
