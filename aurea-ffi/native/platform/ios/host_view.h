#ifndef NATIVE_GUI_IOS_HOST_VIEW_H
#define NATIVE_GUI_IOS_HOST_VIEW_H

#import <UIKit/UIKit.h>
#import "host.h"

/* The one view an Aurea app fills. host.m has the shared part, the input feature files (pen,
   touch, drop, text) take their events from it. */
@class AureaKeyInput;

@interface AureaHostView : UIView <UIPencilInteractionDelegate, UIDropInteractionDelegate>
@property (strong) CADisplayLink* link;
@property (assign) BOOL dirty;
@property (assign) int idle;
@property (strong) CALayer* content;
@property (assign) uint32_t background;
@property (strong) NSMutableArray<CALayer*>* tiles;
@property (strong) UIView* gpuView;
@property (assign) unsigned gridWidth;
@property (assign) unsigned gridHeight;
@property (assign) CGFloat gridScale;
@property (strong) AureaKeyInput* keys;
@property (assign) CGFloat keyboardHeight;
/* The MouseButton a trackpad or mouse click holds, -1 for none. */
@property (assign) int pointerButton;
/* A trackpad or mouse pointer is over the view; a pencil's hover ending is not its exit. */
@property (assign) BOOL pointerOver;
- (void)wakeLink;
- (void)markDirty;
- (NSSet<UIPress*>*)keys:(NSSet<UIPress*>*)presses pressed:(int)pressed;
@end


/* What the app gave ng_ios_run, and the view it runs in. */
extern NGIosHost g_ios_host;
extern __weak AureaHostView* g_ios_view;

#endif
