#ifndef NATIVE_GUI_IOS_PRESENT_H
#define NATIVE_GUI_IOS_PRESENT_H

#import <UIKit/UIKit.h>

/* The view controller on top, for sheets and pickers to present over; nil before there is one. */
UIViewController* ng_ios_presenter(void);

#endif
