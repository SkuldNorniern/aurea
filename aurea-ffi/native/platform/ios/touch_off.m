#import "touch.h"

/* Without touch input fingers reach the app as nothing. */
void ng_ios_touch(UIView* view, int action, UITouch* touch) {
    (void)view;
    (void)action;
    (void)touch;
}
