#import "pen.h"

/* Without pen input a tablet is a mouse and nothing more. */
unsigned int ng_macos_pen_bits(NSEvent* event) {
    (void)event;
    return 0;
}

void ng_macos_pen_report(AureaContentView* view, NSEvent* event, unsigned int phase) {
    (void)view;
    (void)event;
    (void)phase;
}

BOOL ng_macos_pen_proximity(NSEvent* event) {
    (void)event;
    return NO;
}

BOOL ng_macos_pen_point(AureaContentView* view, NSEvent* event) {
    (void)view;
    (void)event;
    return NO;
}
