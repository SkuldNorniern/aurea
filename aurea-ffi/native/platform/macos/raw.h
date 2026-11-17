#ifndef NATIVE_GUI_MACOS_RAW_H
#define NATIVE_GUI_MACOS_RAW_H

#import <Cocoa/Cocoa.h>

@class AureaContentView;

/* Mouse deltas while the cursor is locked. the first one after the lock is the warp, dropped. */
void ng_macos_raw_motion(AureaContentView* view, NSEvent* event);

#endif
