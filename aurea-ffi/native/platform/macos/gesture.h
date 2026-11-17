#ifndef NATIVE_GUI_MACOS_GESTURE_H
#define NATIVE_GUI_MACOS_GESTURE_H

#import <Cocoa/Cocoa.h>

@class AureaContentView;

/* Trackpad pinch and twist, YES when taken. */
BOOL ng_macos_gesture_magnify(AureaContentView* view, NSEvent* event);
BOOL ng_macos_gesture_rotate(AureaContentView* view, NSEvent* event);

#endif
