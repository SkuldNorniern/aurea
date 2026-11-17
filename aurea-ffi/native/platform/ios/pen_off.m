#import "pen.h"

/* Without pen input a pencil is only a finger. */
void ng_ios_pen_attach(UIView* view) {
    (void)view;
}

void ng_ios_pen_touch(UIView* view, int action, UITouch* touch, UIEvent* event) {
    (void)view;
    (void)action;
    (void)touch;
    (void)event;
}

void ng_ios_pen_hover(UIView* view, UIHoverGestureRecognizer* hover) {
    (void)view;
    (void)hover;
}
