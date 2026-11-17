#ifndef NATIVE_GUI_MACOS_DROP_H
#define NATIVE_GUI_MACOS_DROP_H

#import <Cocoa/Cocoa.h>

@class AureaContentView;

/* Lets files be dragged onto `view`. */
void ng_macos_drop_register(AureaContentView* view);
/* Files over the view: reports where, copy when they are files. */
NSDragOperation ng_macos_drop_hover(AureaContentView* view, id<NSDraggingInfo> info);
void ng_macos_drop_leave(AureaContentView* view);
BOOL ng_macos_drop_perform(AureaContentView* view, id<NSDraggingInfo> info);

#endif
