#ifndef NATIVE_GUI_IOS_PEN_H
#define NATIVE_GUI_IOS_PEN_H

#import <UIKit/UIKit.h>

/* Pencil taps and squeezes on `view`. */
void ng_ios_pen_attach(UIView* view);
/* A pencil touch in `view` as pen samples; `action` is one of NG_IOS_TOUCH_*. */
void ng_ios_pen_touch(UIView* view, int action, UITouch* touch, UIEvent* event);
/* A pencil held over `view`. */
void ng_ios_pen_hover(UIView* view, UIHoverGestureRecognizer* hover) API_AVAILABLE(ios(13.0));

#endif
