#ifndef NATIVE_GUI_IOS_TOUCH_H
#define NATIVE_GUI_IOS_TOUCH_H

#import <UIKit/UIKit.h>

/* A finger, or a pencil as one, in `view`; `action` is one of NG_IOS_TOUCH_*. */
void ng_ios_touch(UIView* view, int action, UITouch* touch);

#endif
